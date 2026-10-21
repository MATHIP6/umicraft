// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Held Item Change` (0x09, clientbound, play): the server changed the
//! player's selected hotbar slot.
//!
//! Layout (MCP `S09PacketHeldItemChange`, `readPacketData`): one signed
//! byte — the hotbar index. The reference client ignores values outside
//! `0..9` (`EntityPlayerSP.setCurrentItemOrArmor`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;

/// `Held Item Change` (0x09, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldItemChange {
    /// Hotbar index (0–8); other values are ignored by the reference client.
    pub slot: i8,
}

impl HeldItemChange {
    /// Wire packet id.
    pub const ID: u8 = 0x09;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            slot: read_i8(reader)?,
        })
    }
}

impl EncodablePacket for HeldItemChange {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_item_change_wire_shape() {
        // Captured from a live 1.8 server: a single body byte, slot 0.
        let bytes = [0x00];
        let packet = HeldItemChange::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.slot, 0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn an_out_of_range_slot_stays_negative() {
        // The reference client reads a signed byte, so 0xFF is -1.
        let bytes = [0xFF];
        let packet = HeldItemChange::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.slot, -1);
    }
}
