// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packet hooks of the external mods on the network thread (F3.5, ADR-0086).
//!
//! [`ModPacketFilter`] is the [`PacketFilter`] the play loop runs every packet
//! through: it dispatches `PacketReceived` and `PacketSend` on the network
//! thread's own bus and hands the loop the packets the mods injected. Without
//! a subscriber it copies no body and encodes nothing, and without an
//! injection it only checks that the queue is empty, so a client without
//! packet mods pays two branches per packet.

use umicraft_core::events::EventBus;
use umicraft_modhost::{PacketOutbox, PacketReceived, PacketSend};
use umicraft_net::play::PacketFilter;
use umicraft_protocol::PlayServerbound;

/// What the mods give the network thread of a session: its bus and the queue of
/// injected packets. `NetMods::default()` is a session without mods.
#[derive(Default)]
pub struct NetMods {
    /// The subscriptions of the mods that live on the network thread.
    pub bus: EventBus,
    /// The packets the mods injected; `None` without mods.
    pub outbox: Option<PacketOutbox>,
}

/// The [`PacketFilter`] of a session with mods.
pub(super) struct ModPacketFilter {
    mods: NetMods,
    /// The body of the serverbound packet being dispatched, kept for its buffer.
    scratch: Vec<u8>,
}

impl ModPacketFilter {
    /// Takes the mods of a new session. Injections left from an earlier session
    /// are dropped: they were meant for another server.
    pub(super) fn new(mods: NetMods) -> Self {
        if let Some(outbox) = &mods.outbox {
            outbox.clear();
        }
        Self {
            mods,
            scratch: Vec::new(),
        }
    }
}

impl PacketFilter for ModPacketFilter {
    fn receive(&mut self, id: u8, body: &[u8]) -> bool {
        if !self.mods.bus.has_subscribers::<PacketReceived>() {
            return true;
        }
        !self.mods.bus.dispatch(&mut PacketReceived::new(id, body))
    }

    fn send(&mut self, packet: &PlayServerbound) -> bool {
        if !self.mods.bus.has_subscribers::<PacketSend>() {
            return true;
        }
        self.scratch.clear();
        if let Err(error) = packet.encode_body(&mut self.scratch) {
            // The packet is the client's own; an encoding failure is also what
            // the socket write would hit, so leave the verdict to it.
            tracing::warn!(%error, "cannot encode a packet for the mods");
            return true;
        }
        let mut event = PacketSend::new(packet.packet_id(), &self.scratch);
        !self.mods.bus.dispatch(&mut event)
    }

    fn inject(&mut self, out: &mut Vec<PlayServerbound>) {
        if let Some(outbox) = &self.mods.outbox {
            if !outbox.is_empty() {
                outbox.drain(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use umicraft_core::module::ModuleId;
    use umicraft_protocol::packets::play::serverbound::Chat;

    use super::*;

    fn chat() -> PlayServerbound {
        PlayServerbound::Chat(Chat {
            message: "hi".to_owned(),
        })
    }

    #[test]
    fn without_mods_every_packet_passes_and_nothing_is_injected() {
        let mut filter = ModPacketFilter::new(NetMods::default());
        assert!(filter.receive(0x03, &[0; 16]));
        assert!(filter.send(&chat()));
        let mut out = Vec::new();
        filter.inject(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn a_cancelling_handler_drops_the_packet_in_both_directions() {
        let mut bus = EventBus::default();
        let owner = ModuleId::new("test");
        bus.on::<PacketReceived, _>(owner.clone(), 0, |event, cx| {
            if event.id() == 0x03 {
                cx.cancel();
            }
        });
        bus.on::<PacketSend, _>(owner, 0, |event, cx| {
            // The handler sees the encoded body: a VarInt length then the text.
            if event.id() == 0x01 && event.data() == [2, b'h', b'i'] {
                cx.cancel();
            }
        });
        let mut filter = ModPacketFilter::new(NetMods { bus, outbox: None });
        assert!(!filter.receive(0x03, &[0; 16]));
        assert!(filter.receive(0x00, &[0]));
        assert!(!filter.send(&chat()));
    }
}
