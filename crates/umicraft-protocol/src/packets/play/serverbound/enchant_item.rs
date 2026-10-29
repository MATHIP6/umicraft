// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Enchant Item` (0x11, serverbound, play): the player picked an enchantment
//! in an enchanting table window.
//!
//! Layout (MCP `C11PacketEnchantItem`, `writePacketData`): the window id
//! byte, then the enchantment index byte (0-2).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;

/// `Enchant Item` (0x11, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantItem {
    /// Id of the enchanting window.
    pub window_id: i8,
    /// Index of the chosen enchantment.
    pub enchantment: i8,
}

impl EnchantItem {
    /// Wire packet id.
    pub const ID: u8 = 0x11;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            window_id: read_i8(reader)?,
            enchantment: read_i8(reader)?,
        })
    }
}

impl EncodablePacket for EnchantItem {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.window_id)?;
        write_i8(writer, self.enchantment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enchant_item_wire_shape() {
        let bytes = [0x05, 0x02];
        let packet = EnchantItem::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.window_id, 5);
        assert_eq!(packet.enchantment, 2);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        assert!(EnchantItem::decode(&mut &[0x05][..]).is_err());
    }
}
