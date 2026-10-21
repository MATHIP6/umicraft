// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Held Item Change` (0x09, serverbound, play): the player selected
//! another hotbar slot.
//!
//! Layout (MCP `C09PacketHeldItemChange`, `readPacketData`): a signed
//! 16-bit slot index. The reference server ignores values outside `0..9`
//! (`NetHandlerPlayServer.processHeldItemChange`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i16, write_i16};
use crate::packets::EncodablePacket;

/// `Held Item Change` (0x09, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldItemChange {
    /// Hotbar index (0–8); other values are ignored by the reference server.
    pub slot: i16,
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
            slot: read_i16(reader)?,
        })
    }
}

impl EncodablePacket for HeldItemChange {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i16(writer, self.slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_item_change_wire_shape() {
        // The slot is a 16-bit field, not a byte.
        let bytes = [0x00, 0x08];
        let packet = HeldItemChange::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.slot, 8);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_negative_slot_stays_negative() {
        let bytes = [0xFF, 0xFF];
        let packet = HeldItemChange::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.slot, -1);
    }
}
