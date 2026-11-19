// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed views over the event payloads of the C ABI.

use umicraft_modapi_sys::{UmiEventId, UmiPostTick, UmiPreTick};

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

macro_rules! tick_event {
    ($(#[$doc:meta])* $view:ident, $raw:ty, $id:expr) => {
        $(#[$doc])*
        #[repr(transparent)]
        #[derive(Debug)]
        pub struct $view($raw);

        impl $view {
            /// The tick index carried by the event.
            pub fn tick(&self) -> u64 {
                self.0.tick
            }
        }

        impl sealed::Sealed for $view {}

        impl Event for $view {
            const ID: UmiEventId = $id;
        }
    };
}

tick_event!(
    /// Fired before the client runs a game tick.
    PreTick,
    UmiPreTick,
    UmiEventId::PRE_TICK
);
tick_event!(
    /// Fired after the client ran a game tick.
    PostTick,
    UmiPostTick,
    UmiEventId::POST_TICK
);
