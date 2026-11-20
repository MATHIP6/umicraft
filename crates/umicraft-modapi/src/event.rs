// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed views over the event payloads of the C ABI.

use umicraft_modapi_sys::{
    UmiChatReceived, UmiChatSend, UmiConnected, UmiDisconnected, UmiEventHeader, UmiEventId,
    UmiKeyPressed, UmiPacket, UmiPostTick, UmiPreTick, UmiScreen, UMI_CHAT_POSITION_ACTION_BAR,
    UMI_CHAT_POSITION_CHAT, UMI_CHAT_POSITION_SYSTEM,
};

use crate::ffi::{bytes_of, str_of};
use crate::{host, ModError};

mod sealed {
    /// Implementors must be `#[repr(transparent)]` over a `#[repr(C)]` payload
    /// that starts with a `UmiEventHeader` carrying the event's id, because the
    /// trampoline casts the host's payload pointer to the view. Sealed so only
    /// this crate can uphold that.
    pub trait Sealed {}
}

/// An event a mod can subscribe to. Sealed: the set of events is the host's.
pub trait Event: sealed::Sealed + 'static {
    /// The ABI id the host dispatches this event under.
    const ID: UmiEventId;
}

/// Lets an intercepting handler cancel the event it is looking at.
#[derive(Debug)]
pub struct Control {
    cancellable: bool,
    cancelled: bool,
}

impl Control {
    pub(crate) fn new(cancellable: bool) -> Self {
        Self {
            cancellable,
            cancelled: false,
        }
    }

    /// Cancels the event. A no-op on events that are not cancellable, so
    /// handlers need not know which events allow it.
    pub fn cancel(&mut self) {
        self.cancelled = self.cancellable;
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// Declares a view type: transparent over the ABI payload, so the trampoline
/// can cast the host's pointer to it, and tied to its event id.
macro_rules! view {
    ($(#[$doc:meta])* $view:ident, $raw:ty, $id:expr) => {
        $(#[$doc])*
        #[repr(transparent)]
        #[derive(Debug)]
        pub struct $view($raw);

        impl sealed::Sealed for $view {}

        impl Event for $view {
            const ID: UmiEventId = $id;
        }
    };
}

view!(
    /// Fired before the client runs a game tick.
    PreTick,
    UmiPreTick,
    UmiEventId::PRE_TICK
);
view!(
    /// Fired after the client ran a game tick.
    PostTick,
    UmiPostTick,
    UmiEventId::POST_TICK
);
view!(
    /// A chat line arrived from the server. Cancellable (the line is hidden);
    /// an intercepting handler may replace its JSON.
    ChatReceived,
    UmiChatReceived,
    UmiEventId::CHAT_RECEIVED
);
view!(
    /// The player sends a chat line or command. Cancellable (nothing is sent);
    /// an intercepting handler may replace the line.
    ChatSend,
    UmiChatSend,
    UmiEventId::CHAT_SEND
);
view!(
    /// The player pressed a key binding this mod registered with
    /// [`InitCx::key`](crate::InitCx::key), with no screen open. Delivered to
    /// the registering mod only: the API has no raw key stream, so a mod
    /// cannot read what the player types (ADR-0086).
    KeyPressed,
    UmiKeyPressed,
    UmiEventId::KEY_PRESSED
);
view!(
    /// The client joined a server.
    Connected,
    UmiConnected,
    UmiEventId::CONNECTED
);
view!(
    /// The client left a server or was kicked.
    Disconnected,
    UmiDisconnected,
    UmiEventId::DISCONNECTED
);
view!(
    /// A screen opened.
    ScreenOpened,
    UmiScreen,
    UmiEventId::SCREEN_OPENED
);
view!(
    /// A screen closed.
    ScreenClosed,
    UmiScreen,
    UmiEventId::SCREEN_CLOSED
);
view!(
    /// A play packet arrived from the server, before the client handles it.
    /// Cancellable (the client never sees it), except `Set Compression` and
    /// `Disconnect`, which the host always keeps. Network thread; needs
    /// `network_packets = true` in `mod.toml`.
    PacketReceived,
    UmiPacket,
    UmiEventId::PACKET_RECEIVED
);
view!(
    /// A play packet is about to be written to the server: the client's own,
    /// its automatic replies and packets injected with
    /// [`Packets::send`](crate::Packets::send). Cancellable (nothing is
    /// written). Network thread; needs `network_packets = true` in `mod.toml`.
    PacketSend,
    UmiPacket,
    UmiEventId::PACKET_SEND
);

/// Where a received chat line shows (the protocol's position byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatPosition {
    /// A player chat message.
    Chat,
    /// A system message.
    System,
    /// The action bar above the hotbar.
    ActionBar,
    /// A position this API version does not name; the raw byte.
    Other(u8),
}

impl From<u8> for ChatPosition {
    fn from(raw: u8) -> Self {
        match raw {
            UMI_CHAT_POSITION_CHAT => Self::Chat,
            UMI_CHAT_POSITION_SYSTEM => Self::System,
            UMI_CHAT_POSITION_ACTION_BAR => Self::ActionBar,
            other => Self::Other(other),
        }
    }
}

/// Asks the host to replace the text of the event `view` is a view of.
///
/// The pointer is derived from the whole view, not from its header field, so
/// it keeps the provenance the host needs to write the rest of the payload.
fn replace_text<V: Event>(view: &mut V, text: &str) -> Result<(), ModError> {
    let event = core::ptr::from_mut(view).cast::<UmiEventHeader>();
    // The host is the authority on whether the event is mutable, so the flag
    // is not checked here and cannot disagree with it.
    host::call(|host, _| (host.event_set_text)(event, host::utf8(text))).and_then(ModError::check)
}

impl PreTick {
    /// The index of the tick about to run.
    pub fn tick(&self) -> u64 {
        self.0.tick
    }
}

impl PostTick {
    /// The index of the tick that just ran.
    pub fn tick(&self) -> u64 {
        self.0.tick
    }
}

impl ChatReceived {
    /// The chat component as JSON, exactly as the server sent it (or as an
    /// earlier handler replaced it). Reads as empty if the host sent bad UTF-8.
    pub fn json(&self) -> &str {
        str_of(&self.0.json)
    }

    /// Where the line shows.
    pub fn position(&self) -> ChatPosition {
        self.0.position.into()
    }

    /// Replaces the JSON; later handlers and the client see the new text.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` when called from an observer
    /// (observers get a copy, so the event is not `MUTABLE`).
    pub fn set_json(&mut self, json: &str) -> Result<(), ModError> {
        replace_text(self, json)
    }
}

impl ChatSend {
    /// The line as typed, commands included (`/...`).
    pub fn text(&self) -> &str {
        str_of(&self.0.text)
    }

    /// Replaces the line that will be sent.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` when called from an observer.
    pub fn set_text(&mut self, text: &str) -> Result<(), ModError> {
        replace_text(self, text)
    }
}

impl KeyPressed {
    /// The id the mod gave the binding in [`InitCx::key`](crate::InitCx::key).
    pub fn key(&self) -> &str {
        str_of(&self.0.key)
    }
}

impl Connected {
    /// The server address as the player entered it (`host[:port]`).
    pub fn address(&self) -> &str {
        str_of(&self.0.address)
    }
}

impl Disconnected {
    /// Why the connection ended, as plain text; empty when the player left.
    pub fn reason(&self) -> &str {
        str_of(&self.0.reason)
    }
}

/// Declares `name()` on a screen view; both screen events share one payload.
macro_rules! screen_name {
    ($($view:ident),+) => {$(
        impl $view {
            /// The stable screen name (`chat`, `inventory`, `game_menu`, ...).
            pub fn name(&self) -> &str {
                str_of(&self.0.name)
            }
        }
    )+};
}

screen_name!(ScreenOpened, ScreenClosed);

/// Declares `id()` and `data()` on a packet view; both packet events share one
/// payload.
macro_rules! packet_fields {
    ($($view:ident),+) => {$(
        impl $view {
            /// The play-state packet id (protocol 47). A host sending a value
            /// above 255, which no packet has, reads as `u8::MAX`: that is a
            /// wrong id either way, and it never aliases a real low id the way
            /// truncation would.
            pub fn id(&self) -> u8 {
                u8::try_from(self.0.id).unwrap_or(u8::MAX)
            }

            /// The packet body: no id and no length prefix, never compressed.
            /// Borrowed from the event, so copy it to keep it. Empty for a
            /// packet without a body.
            pub fn data(&self) -> &[u8] {
                bytes_of(&self.0.data)
            }
        }
    )+};
}

packet_fields!(PacketReceived, PacketSend);
