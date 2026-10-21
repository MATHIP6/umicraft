// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Equipment` (0x04, clientbound, play): one equipped item of one
//! entity.
//!
//! Layout (MCP `S04PacketEntityEquipment`, `readPacketData`): VarInt entity
//! id, `i16` equipment slot, then the item stack (see
//! [`crate::packets::play::item_stack`]).
//!
//! The slot is the armor-slot index for armor (0 = feet … 3 = head, 4 =
//! offhand) and the hotbar index for the held item.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::item_stack::ItemStack;
use crate::packets::play::wire::read_i16;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Equipment` (0x04, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityEquipment {
    /// The entity whose equipment changed.
    pub entity_id: i32,
    /// Equipment slot: armor index, or the hotbar index for the held item.
    pub slot: i16,
    /// The item now in that slot.
    pub item: ItemStack,
}

impl EntityEquipment {
    /// Wire packet id.
    pub const ID: u8 = 0x04;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, or a malformed
    /// item stack.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            slot: read_i16(reader)?,
            item: ItemStack::decode(reader)?,
        })
    }
}

impl EncodablePacket for EntityEquipment {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.slot.to_be_bytes())?;
        self.item.encode(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_equipment_wire_shape() {
        let bytes = [
            0xAE, 0xBD, 0x01, // entity id 24238
            0x00, 0x04, // slot 4 (offhand)
            0x00, 0x01, // item id 1 (stone)
            0x40, // stack size 64
            0x00, 0x00, // damage 0
            0x00, // no NBT
        ];
        let packet = EntityEquipment::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);
        assert_eq!(packet.slot, 4);
        assert_eq!(packet.item.item_id, 1);
        assert_eq!(packet.item.stack_size, 64);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn an_unequipped_slot_carries_the_empty_stack() {
        let bytes = [0x01, 0x00, 0x00, 0xFF, 0xFF];
        let packet = EntityEquipment::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.item, ItemStack::EMPTY);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
