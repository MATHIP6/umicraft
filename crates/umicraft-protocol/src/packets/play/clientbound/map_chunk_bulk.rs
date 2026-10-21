// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Map Chunk Bulk` (0x26, clientbound, play): the initial chunk batch sent
//! right after joining.
//!
//! Layout pinned to MCP `S26PacketMapChunkBulk`: `sky_light` bool (1 for
//! overworld), VarInt chunk count, one 10-byte metadata entry per chunk
//! (i32 X, i32 Z, u16 section mask big-endian), then the chunk payloads
//! concatenated **without** individual length prefixes. Each payload uses the
//! `Chunk Data` layout with `ground_up = true`; its length is derived from the
//! mask and the sky-light flag.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_i32};
use crate::packets::play::clientbound::chunk_data::{chunk_payload_len, MAX_CHUNK_DATA_LEN};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// Hard cap on chunks per bulk packet. Keeps the worst-case allocation
/// bounded (64 × ~192 kB) while comfortably covering vanilla batches.
pub const MAX_BULK_CHUNKS: usize = 64;

/// One chunk inside a [`MapChunkBulk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkChunk {
    /// Chunk X coordinate.
    pub chunk_x: i32,
    /// Chunk Z coordinate.
    pub chunk_z: i32,
    /// Bit mask of the 16³ sections present in `data`.
    pub primary_bit_mask: u16,
    /// Raw payload (block states, block light, sky light, biomes).
    pub data: Vec<u8>,
}

/// `Map Chunk Bulk` (0x26, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapChunkBulk {
    /// True when the payloads include sky-light nibbles (overworld).
    pub sky_light: bool,
    /// Chunks in this batch.
    pub chunks: Vec<BulkChunk>,
}

impl MapChunkBulk {
    /// Wire packet id.
    pub const ID: u8 = 0x26;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when the chunk count or a derived payload
    /// length exceeds its budget; [`ProtocolError`] on truncation or
    /// malformed VarInts.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let sky_light = read_bool(reader)?;
        let count = read_var_i32(reader)?;
        let count = usize::try_from(count)
            .map_err(|_| ProtocolError::Invariant(format!("negative bulk chunk count {count}")))?;
        if count > MAX_BULK_CHUNKS {
            return Err(ProtocolError::Invariant(format!(
                "bulk chunk count {count} exceeds budget {MAX_BULK_CHUNKS}"
            )));
        }

        let mut metas = Vec::with_capacity(count);
        for _ in 0..count {
            let chunk_x = read_i32(reader)?;
            let chunk_z = read_i32(reader)?;
            let mut mask = [0u8; 2];
            reader.read_exact(&mut mask)?;
            metas.push((chunk_x, chunk_z, u16::from_be_bytes(mask)));
        }

        let mut chunks = Vec::with_capacity(count);
        for (chunk_x, chunk_z, primary_bit_mask) in metas {
            let len = chunk_payload_len(primary_bit_mask, sky_light, true);
            if len > MAX_CHUNK_DATA_LEN {
                return Err(ProtocolError::Invariant(format!(
                    "bulk chunk payload {} exceeds budget {}",
                    len, MAX_CHUNK_DATA_LEN
                )));
            }
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data)?;
            chunks.push(BulkChunk {
                chunk_x,
                chunk_z,
                primary_bit_mask,
                data,
            });
        }
        Ok(Self { sky_light, chunks })
    }
}

impl EncodablePacket for MapChunkBulk {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[u8::from(self.sky_light)])?;
        write_var_u32(writer, self.chunks.len() as u32)?;
        for chunk in &self.chunks {
            writer.write_all(&chunk.chunk_x.to_be_bytes())?;
            writer.write_all(&chunk.chunk_z.to_be_bytes())?;
            writer.write_all(&chunk.primary_bit_mask.to_be_bytes())?;
        }
        for chunk in &self.chunks {
            writer.write_all(&chunk.data)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulk(sky_light: bool) -> MapChunkBulk {
        let len = chunk_payload_len(1, sky_light, true);
        MapChunkBulk {
            sky_light,
            chunks: vec![
                BulkChunk {
                    chunk_x: -107,
                    chunk_z: 6,
                    primary_bit_mask: 1,
                    data: vec![0; len],
                },
                BulkChunk {
                    chunk_x: -106,
                    chunk_z: 6,
                    primary_bit_mask: 1,
                    data: vec![0; len],
                },
            ],
        }
    }

    #[test]
    fn round_trip_overworld_and_nether_sizes() {
        for sky_light in [true, false] {
            let packet = bulk(sky_light);
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(MapChunkBulk::decode(&mut &buffer[..]).unwrap(), packet);
        }
        // Overworld payloads carry an extra 2048-byte sky-light nibble array.
        let mut with_sky = Vec::new();
        bulk(true).encode_body(&mut with_sky).unwrap();
        let mut without_sky = Vec::new();
        bulk(false).encode_body(&mut without_sky).unwrap();
        assert_eq!(with_sky.len() - without_sky.len(), 2 * 2048);
    }

    #[test]
    fn oversized_chunk_count_is_rejected() {
        let mut buffer = vec![1];
        write_var_u32(&mut buffer, (MAX_BULK_CHUNKS + 1) as u32).unwrap();
        assert!(MapChunkBulk::decode(&mut &buffer[..]).is_err());
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let packet = bulk(true);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        buffer.truncate(buffer.len() - 1);
        assert!(MapChunkBulk::decode(&mut &buffer[..]).is_err());
    }
}
