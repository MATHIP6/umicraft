// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Block Placement` (0x08, serverbound, play): the player placed a
//! block or used an item on a block.
//!
//! Layout (MCP `C08PacketPlayerBlockPlacement`, `readPacketData`): one packed
//! 1.8 block position (i64), a face byte (the index of `EnumFacing`), the
//! held item stack, and three unsigned bytes with the exact point the player
//! clicked, counted in sixteenths of a block.
//!
//! The reference client divides those three bytes by 16 to get the offset
//! inside the block face (`(float)buf.readUnsignedByte() / 16.0F`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::play::facing::Facing;
use crate::packets::play::item_stack::ItemStack;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;

/// `Player Block Placement` (0x08, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerBlockPlacement {
    /// Block X of the targeted block.
    pub x: i32,
    /// Block Y of the targeted block.
    pub y: i32,
    /// Block Z of the targeted block.
    pub z: i32,
    /// The face of the block that was hit.
    pub face: Facing,
    /// The item the player holds.
    pub item: ItemStack,
    /// X offset of the click, in sixteenths of a block.
    pub cursor_x: u8,
    /// Y offset of the click, in sixteenths of a block.
    pub cursor_y: u8,
    /// Z offset of the click, in sixteenths of a block.
    pub cursor_z: u8,
}

impl PlayerBlockPlacement {
    /// Wire packet id.
    pub const ID: u8 = 0x08;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an unknown face byte, or a malformed
    /// item stack.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(crate::packets::codec::read_i64(reader)?);
        let face = read_u8(reader)?;
        let face = Facing::from_code(face)
            .ok_or_else(|| ProtocolError::Invariant(format!("unknown block face {face}")))?;
        let item = ItemStack::decode(reader)?;
        Ok(Self {
            x,
            y,
            z,
            face,
            item,
            cursor_x: read_u8(reader)?,
            cursor_y: read_u8(reader)?,
            cursor_z: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for PlayerBlockPlacement {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.face.code()])?;
        self.item.encode(writer)?;
        writer.write_all(&[self.cursor_x, self.cursor_y, self.cursor_z])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_block_placement_wire_shape() {
        let mut expected = Vec::new();
        expected.extend_from_slice(&pack_block_position(10, 64, -3).to_be_bytes());
        expected.push(2); // face: north
        expected.extend_from_slice(&[0x00, 0x01]); // item id 1 (stone)
        expected.push(0x40); // stack size 64
        expected.extend_from_slice(&[0x00, 0x00]); // damage 0
        expected.push(0x00); // no NBT
        expected.extend([8, 8, 0]); // click at the middle of the top face

        let packet = PlayerBlockPlacement {
            x: 10,
            y: 64,
            z: -3,
            face: Facing::North,
            item: ItemStack {
                item_id: 1,
                stack_size: 64,
                damage: 0,
                nbt: None,
            },
            cursor_x: 8,
            cursor_y: 8,
            cursor_z: 0,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);

        let decoded = PlayerBlockPlacement::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn the_empty_hand_sends_the_empty_stack() {
        let mut expected = Vec::new();
        expected.extend_from_slice(&pack_block_position(0, 0, 0).to_be_bytes());
        expected.push(0); // face: down
        expected.extend_from_slice(&[0xFF, 0xFF]); // empty item stack
        expected.extend([0, 0, 0]);

        let decoded = PlayerBlockPlacement::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded.item, ItemStack::EMPTY);
        assert_eq!(decoded.face, Facing::Down);
    }
}
