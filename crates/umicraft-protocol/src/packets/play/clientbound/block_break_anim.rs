// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Block Break Animation` (0x25, clientbound, play): someone is breaking
//! a block; the crack overlay advances.
//!
//! Layout (MCP `S25PacketBlockBreakAnim`, `readPacketData`): VarInt breaker
//! entity id, one packed 1.8 block position (i64), and an unsigned byte with
//! the break stage (0 = no crack … 9 = almost destroyed).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Block Break Animation` (0x25, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockBreakAnim {
    /// Entity id of the player breaking the block (-1 when unknown).
    pub entity_id: i32,
    /// Block X of the block being broken.
    pub x: i32,
    /// Block Y of the block being broken.
    pub y: i32,
    /// Block Z of the block being broken.
    pub z: i32,
    /// Break stage, 0 (no crack) … 9 (almost destroyed).
    pub stage: u8,
}

impl BlockBreakAnim {
    /// Wire packet id.
    pub const ID: u8 = 0x25;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let (x, y, z) = unpack_block_position(crate::packets::codec::read_i64(reader)?);
        Ok(Self {
            entity_id,
            x,
            y,
            z,
            stage: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for BlockBreakAnim {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.stage])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_break_anim_wire_shape() {
        let mut expected = Vec::new();
        expected.push(0x2A); // entity id 42
        expected.extend_from_slice(&pack_block_position(-5, 70, 12).to_be_bytes());
        expected.push(4); // stage 4

        let packet = BlockBreakAnim {
            entity_id: 42,
            x: -5,
            y: 70,
            z: 12,
            stage: 4,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);

        let decoded = BlockBreakAnim::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded, packet);
    }
}
