// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packet injection: the policy behind `UmiHost::packet_send` and the outbox the
//! client's network thread drains (F3.5).
//!
//! A mod hands over a raw serverbound id and body. Nothing unchecked reaches the
//! connection: the body must decode as exactly one `PlayServerbound` packet, so a
//! mod can send any packet the protocol crate models and nothing it cannot validate.

use crossbeam_channel::Receiver;
use umicraft_modapi_sys::UmiStatus;
use umicraft_protocol::PlayServerbound;

use crate::context::ModContext;
use crate::hook::warn_sparse;

/// Longest body `packet_send` reads from a mod, in bytes (the 1.8 serverbound
/// packets that carry free-form data stay far below this).
pub const MAX_PACKET_BODY_BYTES: usize = 32 * 1024;

/// The packets mods injected, waiting for the network thread.
///
/// `Send + Clone`; all clones share one queue.
#[derive(Debug, Clone)]
pub struct PacketOutbox {
    inbox: Receiver<PlayServerbound>,
}

impl PacketOutbox {
    pub(crate) fn new(inbox: Receiver<PlayServerbound>) -> Self {
        Self { inbox }
    }

    /// Moves every queued packet to the end of `out`, oldest first, without blocking.
    pub fn drain(&self, out: &mut Vec<PlayServerbound>) {
        out.extend(self.inbox.try_iter());
    }

    /// Discards every queued packet, so a stale injection never reaches the next
    /// server.
    pub fn clear(&self) {
        while self.inbox.try_recv().is_ok() {}
    }

    /// Whether nothing is queued; cheap enough to ask every loop turn.
    pub fn is_empty(&self) -> bool {
        self.inbox.is_empty()
    }
}

/// Decodes `body` as exactly one serverbound packet `id`, `None` if it is not one.
fn decode(id: u32, body: &[u8]) -> Option<PlayServerbound> {
    let id = u8::try_from(id).ok()?;
    let mut rest = body;
    let packet = PlayServerbound::decode(id, &mut rest).ok()?;
    rest.is_empty().then_some(packet)
}

impl ModContext {
    /// Queues a serverbound play packet for the network thread.
    ///
    /// Returns `DENIED` without the `network_packets` permission, before `init`
    /// returned or while the mod is not enabled; `INVALID_ARGUMENT` for an id above
    /// 0xff or a body that does not decode as exactly one packet of that id; `BUSY`
    /// when the queue is full (the packet is not sent). Callable from any thread.
    pub fn packet_send(&self, id: u32, body: &[u8]) -> UmiStatus {
        if !self.permissions().network_packets || !self.is_enabled() {
            return UmiStatus::DENIED;
        }
        let Some(packet) = decode(id, body) else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        match self.shared.queue_packet(packet) {
            Ok(()) => {
                self.counters().record_injection();
                UmiStatus::OK
            }
            Err(_) => {
                let refused = self.counters().record_drop();
                warn_sparse(self, refused, "packet outbox full, packet refused");
                UmiStatus::BUSY
            }
        }
    }
}

#[cfg(test)]
mod tests;
