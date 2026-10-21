// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Chunk Data` (0x21, clientbound, play): one chunk column, full or partial.
//!
//! Layout pinned to MCP `S21PacketChunkData`: i32 chunk X, i32 chunk Z,
//! `ground_up` bool, u16 primary bit mask (big-endian, one bit per 16³
//! section), then a VarInt-length-prefixed byte array. The array is laid out
//! as: block states (2 bytes/block, little-endian) for every set mask bit,
//! block-light nibbles (2048 bytes/section), sky-light nibbles
//! (2048 bytes/section, overworld only), and — when `ground_up` — the 256
//! biome bytes. A `ground_up` packet with an empty mask unloads the column
//! (MCP `NetHandlerPlayClient.handleChunkData`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_byte_array, read_i32, write_byte_array};
use crate::packets::EncodablePacket;

/// Maximum payload of a 1.8 chunk column: 16 sections with both light arrays
/// plus the biome array. Anything larger is rejected as malformed.
pub const MAX_CHUNK_DATA_LEN: usize = chunk_payload_len(0xFFFF, true, true);

/// Wire byte length of a chunk payload for a given section mask.
///
/// `primary_bit_mask` selects 16³ sections, `has_sky_light` includes the
/// second nibble array, and `ground_up` appends the 256-byte biome array.
#[must_use]
pub const fn chunk_payload_len(
    primary_bit_mask: u16,
    has_sky_light: bool,
    ground_up: bool,
) -> usize {
    let sections = primary_bit_mask.count_ones() as usize;
    let per_section = 2 * 4096 + 2048 + if has_sky_light { 2048 } else { 0 };
    sections * per_section + if ground_up { 256 } else { 0 }
}

/// `Chunk Data` (0x21, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkData {
    /// Chunk X coordinate (block X >> 4).
    pub chunk_x: i32,
    /// Chunk Z coordinate (block Z >> 4).
    pub chunk_z: i32,
    /// True for a full column (replaces it, carries biomes); false for a
    /// partial section update.
    pub ground_up: bool,
    /// Bit mask of the 16³ sections present in `data`.
    pub primary_bit_mask: u16,
    /// Raw payload in the layout described in the module docs.
    pub data: Vec<u8>,
}

impl ChunkData {
    /// Wire packet id.
    pub const ID: u8 = 0x21;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when the payload exceeds
    /// [`MAX_CHUNK_DATA_LEN`]; [`ProtocolError`] on truncation or malformed
    /// VarInts.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let chunk_x = read_i32(reader)?;
        let chunk_z = read_i32(reader)?;
        let ground_up = read_bool(reader)?;
        let mut mask = [0u8; 2];
        reader.read_exact(&mut mask)?;
        let data = read_byte_array(reader, MAX_CHUNK_DATA_LEN)?;
        Ok(Self {
            chunk_x,
            chunk_z,
            ground_up,
            primary_bit_mask: u16::from_be_bytes(mask),
            data,
        })
    }
}

impl EncodablePacket for ChunkData {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.chunk_x.to_be_bytes())?;
        writer.write_all(&self.chunk_z.to_be_bytes())?;
        writer.write_all(&[u8::from(self.ground_up)])?;
        writer.write_all(&self.primary_bit_mask.to_be_bytes())?;
        write_byte_array(writer, &self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_data_round_trip_and_shape() {
        let packet = ChunkData {
            chunk_x: -107,
            chunk_z: 6,
            ground_up: true,
            primary_bit_mask: 0x0001,
            data: vec![0xAA, 0x55, 0x01],
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        // i32 x, i32 z, bool, u16 mask, VarInt length, payload.
        assert_eq!(&buffer[..2], &(-107i32).to_be_bytes()[..2]);
        assert_eq!(buffer[8], 1);
        assert_eq!(&buffer[9..11], &[0x00, 0x01]);
        assert_eq!(buffer[11], 3);
        assert_eq!(ChunkData::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn payload_lengths_match_the_1_8_layout() {
        // One section, no sky light, partial update: 8192 + 2048.
        assert_eq!(chunk_payload_len(0b1, false, false), 10_240);
        // One section, sky light, full column: 8192 + 2048 + 2048 + 256.
        assert_eq!(chunk_payload_len(0b1, true, true), 12_544);
        // Sixteen sections, sky light, full column.
        assert_eq!(MAX_CHUNK_DATA_LEN, 16 * 12_288 + 256);
    }

    #[test]
    fn oversized_payload_is_rejected() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&0i32.to_be_bytes());
        buffer.extend_from_slice(&0i32.to_be_bytes());
        buffer.push(1);
        buffer.extend_from_slice(&[0xFF, 0xFF]);
        // VarInt(MAX_CHUNK_DATA_LEN + 1) then no bytes needed: the budget
        // check fires before reading the body.
        let mut len = [0u8; 5];
        let mut value = (MAX_CHUNK_DATA_LEN + 1) as u32;
        let mut index = 0;
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            len[index] = byte;
            index += 1;
            if value == 0 {
                break;
            }
        }
        buffer.extend_from_slice(&len[..index]);
        let err = ChunkData::decode(&mut &buffer[..]).unwrap_err();
        assert!(matches!(err, ProtocolError::Invariant(_)));
    }

    #[test]
    fn mask_is_big_endian_on_the_wire() {
        let mut body = Vec::new();
        body.extend_from_slice(&0i32.to_be_bytes());
        body.extend_from_slice(&0i32.to_be_bytes());
        body.push(0);
        body.extend_from_slice(&[0x01, 0x00]); // mask 0x0100 = section 8
        body.push(0);
        let packet = ChunkData::decode(&mut &body[..]).unwrap();
        assert_eq!(packet.primary_bit_mask, 0x0100);
    }
}
