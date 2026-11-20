// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network events, emitted on the network thread: a play-state packet the
//! server sent ([`PacketReceived`]) and one the client is about to write
//! ([`PacketSend`]). Both carry the raw id and body (protocol 47, no length prefix,
//! never compressed), are cancellable and are not mutable (F3.5).

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiPacket};

use super::{header_for, host_event, umi_bytes, EventThread, HostEvent};

/// Defines a packet event: payload, owned body, constructor, accessors and a
/// `Clone` that repoints the payload at the clone's own buffer.
macro_rules! packet_event {
    ($(#[$doc:meta])* $name:ident = $id:ident) => {
        $(#[$doc])*
        #[repr(C)]
        #[derive(Debug)]
        pub struct $name {
            pub(crate) raw: UmiPacket,
            body: Vec<u8>,
        }

        impl $name {
            /// Builds the event for packet `id`, copying `body` into the event.
            pub fn new(id: u8, body: &[u8]) -> Self {
                let body = body.to_vec();
                Self {
                    raw: UmiPacket {
                        header: header_for::<UmiPacket>(UmiEventId::$id),
                        id: u32::from(id),
                        data: umi_bytes(&body),
                    },
                    body,
                }
            }

            /// The packet id.
            pub fn id(&self) -> u8 {
                // Built from a `u8` and never written by mods (the event is not mutable).
                self.raw.id as u8
            }

            /// The packet body, without id or length prefix.
            pub fn data(&self) -> &[u8] {
                &self.body
            }
        }

        // Not derived: a derived clone would keep pointing at the original's buffer.
        impl Clone for $name {
            fn clone(&self) -> Self {
                Self::new(self.id(), &self.body)
            }
        }

        host_event!($name, $id, Network, cancellable: true, mutable: false);
    };
}

packet_event! {
    /// A packet arrived from the server. Cancelling hides it from the session, except
    /// `Set Compression` and `Disconnect`, which the network layer always keeps.
    PacketReceived = PACKET_RECEIVED
}

packet_event! {
    /// The client is about to write a packet (commands, automatic replies and mod
    /// injections alike). Cancelling writes nothing.
    PacketSend = PACKET_SEND
}
