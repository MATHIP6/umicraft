// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tick events, emitted on the game thread around each simulation tick.

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiPostTick, UmiPreTick};

use super::{header_for, host_event, EventThread, HostEvent};

/// Defines a tick event around its payload: no strings, so a derived `Clone` is
/// correct.
macro_rules! tick_event {
    ($(#[$doc:meta])* $name:ident($raw:ident) = $id:ident) => {
        $(#[$doc])*
        #[repr(C)]
        #[derive(Debug, Clone)]
        pub struct $name {
            pub(crate) raw: $raw,
        }

        impl $name {
            /// Builds the event for game tick number `tick`.
            pub fn new(tick: u64) -> Self {
                Self {
                    raw: $raw {
                        header: header_for::<$raw>(UmiEventId::$id),
                        tick,
                    },
                }
            }

            /// The tick index.
            pub fn tick(&self) -> u64 {
                self.raw.tick
            }
        }

        host_event!($name, $id, Game, cancellable: false, mutable: false);
    };
}

tick_event! {
    /// Fired before the client runs a game tick.
    PreTick(UmiPreTick) = PRE_TICK
}
tick_event! {
    /// Fired after the client ran a game tick.
    PostTick(UmiPostTick) = POST_TICK
}
