// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Multi Block Change` (0x22, clientbound, play): a batch of block updates
//! inside one chunk column.
//!
//! Layout pinned to MCP `S22PacketMultiBlockChange`: i32 chunk X, i32 chunk Z,
//! VarInt record count, then per record a big-endian u16 packed offset and a
//! VarInt block state. The offset is `x << 12 | z << 8 | y` with 4-bit x/z and
//! 8-bit y (MCP `BlockUpdateData.getPos`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// Hard cap on records per packet; a whole chunk column only has 32 768
/// blocks, and real servers batch far less.
pub const MAX_MULTI_BLOCK_RECORDS: usize = 32_768;

/// One update inside a [`MultiBlockChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockUpdate {
    /// Packed offset `x << 12 | z << 8 | y` within the chunk.
    pub packed_offset: u16,
    /// 1.8 global block state id (`block id << 4 | metadata`).
    pub block_state: u16,
}

impl BlockUpdate {
    /// Creates an update from chunk-local coordinates (0..=15, 0..=255).
    #[must_use]
    pub const fn at(x: u8, y: u8, z: u8, block_state: u16) -> Self {
        Self {
            packed_offset: ((x as u16 & 15) << 12) | ((z as u16 & 15) << 8) | (y as u16),
            block_state,
        }
    }

    /// Chunk-local `(x, y, z)` of this update.
    #[must_use]
    pub const fn local_position(&self) -> (u8, u8, u8) {
        (
            ((self.packed_offset >> 12) & 15) as u8,
            (self.packed_offset & 255) as u8,
            ((self.packed_offset >> 8) & 15) as u8,
        )
    }
}

/// `Multi Block Change` (0x22, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBlockChange {
    /// Chunk X coordinate.
    pub chunk_x: i32,
    /// Chunk Z coordinate.
    pub chunk_z: i32,
    /// Updates in this batch.
    pub records: Vec<BlockUpdate>,
}

impl MultiBlockChange {
    /// Wire packet id.
    pub const ID: u8 = 0x22;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when the count or a state exceeds its
    /// budget; [`ProtocolError`] on truncation or malformed VarInts.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let chunk_x = read_i32(reader)?;
        let chunk_z = read_i32(reader)?;
        let count = read_var_i32(reader)?;
        let count = usize::try_from(count).map_err(|_| {
            ProtocolError::Invariant(format!("negative multi block record count {count}"))
        })?;
        if count > MAX_MULTI_BLOCK_RECORDS {
            return Err(ProtocolError::Invariant(format!(
                "multi block record count {count} exceeds budget {MAX_MULTI_BLOCK_RECORDS}"
            )));
        }
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let mut offset = [0u8; 2];
            reader.read_exact(&mut offset)?;
            let block_state = read_var_i32(reader)?;
            let block_state = u16::try_from(block_state).map_err(|_| {
                ProtocolError::Invariant(format!("block state {block_state} exceeds 16 bits"))
            })?;
            records.push(BlockUpdate {
                packed_offset: u16::from_be_bytes(offset),
                block_state,
            });
        }
        Ok(Self {
            chunk_x,
            chunk_z,
            records,
        })
    }
}

impl EncodablePacket for MultiBlockChange {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.chunk_x.to_be_bytes())?;
        writer.write_all(&self.chunk_z.to_be_bytes())?;
        write_var_u32(writer, self.records.len() as u32)?;
        for record in &self.records {
            writer.write_all(&record.packed_offset.to_be_bytes())?;
            write_var_u32(writer, u32::from(record.block_state))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_layout_matches_mcp() {
        let update = BlockUpdate::at(15, 255, 3, 0x20);
        assert_eq!(update.packed_offset, (15 << 12) | (3 << 8) | 255);
        assert_eq!(update.local_position(), (15, 255, 3));
    }

    #[test]
    fn round_trip() {
        let packet = MultiBlockChange {
            chunk_x: -107,
            chunk_z: 6,
            records: vec![
                BlockUpdate::at(1, 4, 2, 0x20),
                BlockUpdate::at(15, 255, 0, 0),
            ],
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(MultiBlockChange::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn oversized_record_count_is_rejected() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&0i32.to_be_bytes());
        buffer.extend_from_slice(&0i32.to_be_bytes());
        write_var_u32(&mut buffer, (MAX_MULTI_BLOCK_RECORDS + 1) as u32).unwrap();
        assert!(MultiBlockChange::decode(&mut &buffer[..]).is_err());
    }
}
