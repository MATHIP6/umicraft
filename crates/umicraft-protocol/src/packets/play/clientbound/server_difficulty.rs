// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Server Difficulty` (0x41, clientbound, play): the world difficulty.
//!
//! Layout (MCP `S41PacketServerDifficulty`, `readPacketData`): one unsigned
//! byte (0 peaceful, 1 easy, 2 normal, 3 hard). The reference client maps
//! out-of-range values with `id & 3`; the raw byte is kept here.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;

/// `Server Difficulty` (0x41, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerDifficulty {
    /// Difficulty id: 0 peaceful, 1 easy, 2 normal, 3 hard.
    pub difficulty: u8,
}

impl ServerDifficulty {
    /// Wire packet id.
    pub const ID: u8 = 0x41;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            difficulty: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for ServerDifficulty {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.difficulty])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_difficulty_wire_shape() {
        let mut buffer = Vec::new();
        ServerDifficulty { difficulty: 2 }
            .encode_body(&mut buffer)
            .unwrap();
        assert_eq!(buffer, [2]);
        assert_eq!(
            ServerDifficulty::decode(&mut &[3][..]).unwrap().difficulty,
            3
        );
    }

    #[test]
    fn a_truncated_server_difficulty_is_an_error() {
        assert!(ServerDifficulty::decode(&mut &[][..]).is_err());
    }
}
