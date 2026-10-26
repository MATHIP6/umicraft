// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Set Slot` (0x2F, clientbound, play): the server updates a single slot.
//!
//! Layout (MCP `S2FPacketSetSlot`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | i8 | `-1` means the cursor item |
//! | slot | i16 | `-1` with `window_id == -1` is the cursor slot |
//! | item | ItemStack | the new content of the slot |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i16, write_i16};
use crate::packets::play::ItemStack;
use crate::packets::EncodablePacket;

/// `Set Slot` (0x2F, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSlot {
    /// The window id; `-1` targets the cursor item.
    pub window_id: i8,
    /// The slot index within that window; `-1` for the cursor.
    pub slot: i16,
    /// The item placed in the slot.
    pub item: ItemStack,
}

impl SetSlot {
    /// Wire packet id.
    pub const ID: u8 = 0x2F;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation,
    /// [`ProtocolError::Invariant`] on invalid NBT.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let window_id = byte[0] as i8;
        let slot = read_i16(reader)?;
        let item = ItemStack::decode(reader)?;
        Ok(Self {
            window_id,
            slot,
            item,
        })
    }
}

impl EncodablePacket for SetSlot {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&(self.window_id as u8).to_be_bytes())?;
        write_i16(writer, self.slot)?;
        self.item.encode(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &SetSlot) -> SetSlot {
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        SetSlot::decode(&mut &buf[..]).unwrap()
    }

    #[test]
    fn a_normal_slot_round_trips() {
        let packet = SetSlot {
            window_id: 0,
            slot: 36,
            item: ItemStack {
                item_id: 1,
                stack_size: 64,
                damage: 0,
                nbt: None,
            },
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn the_cursor_slot_uses_window_id_minus_one() {
        let packet = SetSlot {
            window_id: -1,
            slot: -1,
            item: ItemStack::EMPTY,
        };
        assert_eq!(round_trip(&packet), packet);
    }
}
