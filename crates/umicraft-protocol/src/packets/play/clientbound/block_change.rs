// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Block Change` (0x23, clientbound, play): one block update.
//!
//! Layout pinned to MCP `S23PacketBlockChange`: packed 1.8 block position
//! (`BlockPos.toLong`: 26-bit X at 38, 12-bit Y at 26, 26-bit Z at 0), then
//! the block state as a VarInt. The state value is the 1.8 global state id
//! `block id << 4 | metadata` (`Block.BLOCK_STATE_IDS`), so it always fits in
//! 16 bits.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::play::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Block Change` (0x23, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockChange {
    /// Packed block position (use [`unpack_block_position`] on the decoder side).
    pub packed_position: i64,
    /// 1.8 global block state id (`block id << 4 | metadata`).
    pub block_state: u16,
}

impl BlockChange {
    /// Wire packet id.
    pub const ID: u8 = 0x23;

    /// Creates an update for the given block coordinates.
    #[must_use]
    pub const fn at(x: i32, y: i32, z: i32, block_state: u16) -> Self {
        Self {
            packed_position: pack_block_position(x, y, z),
            block_state,
        }
    }

    /// Block coordinates of this update.
    #[must_use]
    pub const fn position(&self) -> (i32, i32, i32) {
        unpack_block_position(self.packed_position)
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when the state does not fit in 16 bits;
    /// [`ProtocolError`] on truncation or malformed VarInts.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let packed_position = read_i64(reader)?;
        let block_state = read_var_i32(reader)?;
        let block_state = u16::try_from(block_state).map_err(|_| {
            ProtocolError::Invariant(format!("block state {block_state} exceeds 16 bits"))
        })?;
        Ok(Self {
            packed_position,
            block_state,
        })
    }
}

impl EncodablePacket for BlockChange {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.packed_position.to_be_bytes())?;
        write_var_u32(writer, u32::from(self.block_state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_negative_coordinates() {
        let packet = BlockChange::at(-107 * 16 + 3, 64, -42, 0x1234);
        assert_eq!(packet.position(), (-107 * 16 + 3, 64, -42));
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(BlockChange::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn state_wider_than_16_bits_is_rejected() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&0i64.to_be_bytes());
        write_var_u32(&mut buffer, 0x1_0000).unwrap();
        assert!(BlockChange::decode(&mut &buffer[..]).is_err());
    }
}
