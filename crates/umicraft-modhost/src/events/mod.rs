// SPDX-License-Identifier: MIT OR Apache-2.0

//! The host's event types: the ABI payloads of `umicraft-modapi-sys` plus the
//! Rust-owned data they point into (ADR-0086).
//!
//! Every event is a `#[repr(C)]` struct whose **first field is the ABI payload**,
//! so a pointer to the event is also a pointer to its payload; the host hands that
//! pointer to mods and `event_set_text` casts it back. The payload's `UmiStr`s point
//! into the `String`s the struct owns: a `String`'s heap buffer does not move when
//! the struct does, so the pointers survive being passed around by value. The
//! `String` stays the source of truth, which keeps every accessor safe.
//!
//! Built-in Rust modules subscribe to these same types on the core `EventBus`.

mod chat;
mod key;
mod packet;
mod screen;
mod session;
mod tick;

use std::mem::size_of;

use umicraft_modapi_sys::{UmiBytes, UmiEventHeader, UmiEventId, UmiStr, UMI_EVENT_CANCELLED};

use crate::manifest::ModId;

pub use chat::{ChatReceived, ChatSend};
pub use key::KeyPressed;
pub use packet::{PacketReceived, PacketSend};
pub use screen::{ScreenClosed, ScreenOpened};
pub use session::{Connected, Disconnected};
pub use tick::{PostTick, PreTick};

/// The thread that owns an event, and so the `EventBus` it is dispatched on.
///
/// Each thread has its own bus and no lock (ADR-0086, thread ownership).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventThread {
    /// The 20 TPS game thread.
    Game,
    /// The render thread, which never waits for a mod.
    Render,
    /// The network thread, which reads and writes the connection.
    Network,
}

/// What the dispatcher needs to know about an ABI event type.
///
/// Implemented by the eleven event types of this module only: the `Send` impls and
/// `event_set_text` rely on the host being the sole builder of these events.
pub trait HostEvent: Clone + Send + 'static {
    /// The event id of the payload.
    const ID: UmiEventId;
    /// The thread whose bus carries the event.
    const THREAD: EventThread;
    /// Whether an intercepting handler may cancel the event.
    const CANCELLABLE: bool;
    /// Whether an intercepting handler may change the event (`UMI_EVENT_MUTABLE`).
    const MUTABLE: bool;

    /// The payload header.
    fn header(&self) -> &UmiEventHeader;

    /// The payload header, for the dispatcher to set and clear flags.
    fn header_mut(&mut self) -> &mut UmiEventHeader;

    /// Pointer to the whole event, typed as its payload header.
    ///
    /// Derived from the whole struct, not from the header field, so the mod (and
    /// `event_set_text`) may legitimately reach the payload and the owned strings.
    fn header_ptr(&mut self) -> *mut UmiEventHeader {
        std::ptr::from_mut(self).cast()
    }

    /// The only mod the event is for, or `None` when it goes to every subscriber.
    ///
    /// Both dispatch adapters (intercept and observe) check it, so an event aimed at
    /// one mod never reaches another mod's handler.
    fn target(&self) -> Option<&ModId> {
        None
    }

    /// Whether a handler cancelled the event.
    fn is_cancelled(&self) -> bool {
        self.header().flags & UMI_EVENT_CANCELLED != 0
    }
}

/// Whether `id` names one of the events of this API version.
pub(crate) fn is_known(id: UmiEventId) -> bool {
    matches!(
        id,
        UmiEventId::PRE_TICK
            | UmiEventId::POST_TICK
            | UmiEventId::CHAT_RECEIVED
            | UmiEventId::CHAT_SEND
            | UmiEventId::CONNECTED
            | UmiEventId::DISCONNECTED
            | UmiEventId::SCREEN_OPENED
            | UmiEventId::SCREEN_CLOSED
            | UmiEventId::KEY_PRESSED
            | UmiEventId::PACKET_RECEIVED
            | UmiEventId::PACKET_SEND
    )
}

/// A fresh header for payload type `P`: no flags, `size` of the ABI struct.
fn header_for<P>(id: UmiEventId) -> UmiEventHeader {
    UmiEventHeader {
        size: size_of::<P>() as u32,
        id,
        flags: 0,
    }
}

/// An ABI view of `text`, valid while `text` is not changed or dropped.
pub(crate) fn umi_str(text: &str) -> UmiStr {
    UmiStr {
        ptr: text.as_ptr(),
        len: text.len(),
    }
}

/// An ABI view of `bytes`, valid while `bytes` is not changed or dropped. An empty
/// slice becomes the null view the ABI documents for zero bytes.
pub(crate) fn umi_bytes(bytes: &[u8]) -> UmiBytes {
    if bytes.is_empty() {
        return UmiBytes::EMPTY;
    }
    UmiBytes {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }
}

/// Implements [`HostEvent`] for a struct whose `raw` field is the ABI payload.
macro_rules! host_event {
    ($name:ident, $id:ident, $thread:ident, cancellable: $cancellable:literal, mutable: $mutable:literal) => {
        impl HostEvent for $name {
            const ID: UmiEventId = UmiEventId::$id;
            const THREAD: EventThread = EventThread::$thread;
            const CANCELLABLE: bool = $cancellable;
            const MUTABLE: bool = $mutable;

            fn header(&self) -> &UmiEventHeader {
                &self.raw.header
            }

            fn header_mut(&mut self) -> &mut UmiEventHeader {
                &mut self.raw.header
            }
        }
    };
}

/// Defines an event that is one read-only string: payload, owner, constructor,
/// accessor and a `Clone` that repoints the payload at the clone's own buffer.
macro_rules! text_event {
    (
        $(#[$doc:meta])*
        $name:ident($raw:ident.$field:ident) = $id:ident on $thread:ident
    ) => {
        $(#[$doc])*
        #[repr(C)]
        #[derive(Debug)]
        pub struct $name {
            pub(crate) raw: umicraft_modapi_sys::$raw,
            $field: String,
        }

        impl $name {
            /// Builds the event, copying the text into the event.
            pub fn new($field: &str) -> Self {
                let $field = $field.to_owned();
                Self {
                    raw: umicraft_modapi_sys::$raw {
                        header: header_for::<umicraft_modapi_sys::$raw>(UmiEventId::$id),
                        $field: umi_str(&$field),
                    },
                    $field,
                }
            }

            /// The text of the event.
            pub fn $field(&self) -> &str {
                &self.$field
            }
        }

        // Not derived: a derived clone would copy the `UmiStr` and keep pointing at
        // the original's buffer.
        impl Clone for $name {
            fn clone(&self) -> Self {
                Self::new(&self.$field)
            }
        }

        host_event!($name, $id, $thread, cancellable: false, mutable: false);
    };
}
pub(crate) use {host_event, text_event};

#[cfg(test)]
mod tests;
