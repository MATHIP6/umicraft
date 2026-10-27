// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Block Entity` (0x35, clientbound, play): new NBT data for a block
//! entity (sign, skull, banner, mob spawner, ...).
//!
//! Layout (MCP `S35PacketUpdateTileEntity`, `readPacketData`): packed block
//! position (i64), action id as an unsigned byte (what kind of block entity
//! the update is for: 1 spawner, 2 command block, 3 beacon, 4 skull, ...) and
//! an optional NBT compound (`readNBTTagCompoundFromBuffer`, a single `0x00`
//! when absent). The compound is validated and kept as raw bytes.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_i64, read_u8};
use crate::packets::play::nbt::read_optional_nbt;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;

/// `Update Block Entity` (0x35, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateBlockEntity {
    /// Block X of the block entity.
    pub x: i32,
    /// Block Y of the block entity.
    pub y: i32,
    /// Block Z of the block entity.
    pub z: i32,
    /// Block entity kind of the update.
    pub action: u8,
    /// Raw NBT compound bytes, or `None` when the server sent none.
    pub nbt: Option<Vec<u8>>,
}

impl UpdateBlockEntity {
    /// Wire packet id.
    pub const ID: u8 = 0x35;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or malformed NBT.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        Ok(Self {
            x,
            y,
            z,
            action: read_u8(reader)?,
            nbt: read_optional_nbt(reader)?,
        })
    }
}

impl EncodablePacket for UpdateBlockEntity {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.action])?;
        match &self.nbt {
            Some(bytes) => writer.write_all(bytes)?,
            None => writer.write_all(&[0x00])?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty named compound: tag 10, empty name, end tag.
    const EMPTY_COMPOUND: [u8; 4] = [0x0A, 0x00, 0x00, 0x00];

    #[test]
    fn update_block_entity_wire_shape() {
        let mut expected = pack_block_position(1, 70, 2).to_be_bytes().to_vec();
        expected.push(9);
        expected.extend(EMPTY_COMPOUND);
        let packet = UpdateBlockEntity {
            x: 1,
            y: 70,
            z: 2,
            action: 9,
            nbt: Some(EMPTY_COMPOUND.to_vec()),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            UpdateBlockEntity::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn absent_nbt_is_a_single_zero_byte() {
        let packet = UpdateBlockEntity {
            x: 0,
            y: 0,
            z: 0,
            action: 1,
            nbt: None,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer.len(), 10);
        assert_eq!(UpdateBlockEntity::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn truncated_or_malformed_updates_are_errors() {
        assert!(UpdateBlockEntity::decode(&mut &[][..]).is_err());
        let mut bad = vec![0u8; 8];
        bad.extend([1, 0x0A, 0x00]); // compound cut off after the name length
        assert!(UpdateBlockEntity::decode(&mut &bad[..]).is_err());
    }
}
