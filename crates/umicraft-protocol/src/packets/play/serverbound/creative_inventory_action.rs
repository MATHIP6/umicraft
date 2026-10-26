// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Creative Inventory Action` (0x10, serverbound, play): the client sets a
//! slot in creative mode.
//!
//! Layout (MCP `C10PacketCreativeInventoryAction`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | slot | i16 | |
//! | item | ItemStack | |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i16, write_i16};
use crate::packets::play::ItemStack;
use crate::packets::EncodablePacket;

/// `Creative Inventory Action` (0x10, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreativeInventoryAction {
    /// The slot to set.
    pub slot: i16,
    /// The item to place in the slot.
    pub item: ItemStack,
}

impl CreativeInventoryAction {
    /// Wire packet id.
    pub const ID: u8 = 0x10;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation,
    /// [`ProtocolError::Invariant`] on invalid NBT.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let slot = read_i16(reader)?;
        let item = ItemStack::decode(reader)?;
        Ok(Self { slot, item })
    }
}

impl EncodablePacket for CreativeInventoryAction {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i16(writer, self.slot)?;
        self.item.encode(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creative_inventory_action_round_trips() {
        let packet = CreativeInventoryAction {
            slot: 36,
            item: ItemStack {
                item_id: 276,
                stack_size: 1,
                damage: 0,
                nbt: None,
            },
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(
            CreativeInventoryAction::decode(&mut &buf[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn an_empty_creative_slot_round_trips() {
        let packet = CreativeInventoryAction {
            slot: -1,
            item: ItemStack::EMPTY,
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(
            CreativeInventoryAction::decode(&mut &buf[..]).unwrap(),
            packet
        );
    }
}
