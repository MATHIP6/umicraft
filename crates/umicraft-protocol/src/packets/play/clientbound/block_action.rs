// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Block Action` (0x24, clientbound, play): a block was used — a note
//! block played, a chest opened, a piston moved.
//!
//! Layout (MCP `S24PacketBlockAction`, `readPacketData`): one packed 1.8
//! block position (i64), an unsigned byte "instrument" id, an unsigned byte
//! "pitch" value, and a VarInt with the block id (masked to 12 bits by the
//! reference client).
//!
//! The two bytes are named after their note-block meaning: `instrument` is
//! the note-block instrument (0 = piano … 12 = guitar) and `pitch` is the
//! note (0 … 24). For other blocks the server sends the block-specific
//! values (chest: 0 = closed, 1 = upper lid, 2 = lower lid; piston: the
//! extension state).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Block Action` (0x24, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockAction {
    /// Block X of the affected block.
    pub x: i32,
    /// Block Y of the affected block.
    pub y: i32,
    /// Block Z of the affected block.
    pub z: i32,
    /// Instrument id (note blocks) or the block-specific action id.
    pub instrument: u8,
    /// Pitch (note blocks) or the block-specific action id.
    pub pitch: u8,
    /// Numeric id of the block that produced the action.
    pub block_id: i32,
}

impl BlockAction {
    /// Wire packet id.
    pub const ID: u8 = 0x24;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(crate::packets::codec::read_i64(reader)?);
        Ok(Self {
            x,
            y,
            z,
            instrument: read_u8(reader)?,
            pitch: read_u8(reader)?,
            block_id: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for BlockAction {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.instrument, self.pitch])?;
        write_var_u32(writer, self.block_id as u32)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_action_wire_shape() {
        let mut expected = Vec::new();
        expected.extend_from_slice(&pack_block_position(10, 64, -3).to_be_bytes());
        expected.extend([1, 12]); // instrument, pitch
        expected.push(54); // block id 54 (chest) as a VarInt

        let packet = BlockAction {
            x: 10,
            y: 64,
            z: -3,
            instrument: 1,
            pitch: 12,
            block_id: 54,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);

        let decoded = BlockAction::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn a_truncated_block_action_is_an_error_not_a_panic() {
        assert!(BlockAction::decode(&mut &[][..]).is_err());
        assert!(BlockAction::decode(&mut &[0x00; 10][..]).is_err());
    }
}
