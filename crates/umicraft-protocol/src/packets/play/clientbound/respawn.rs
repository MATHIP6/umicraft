// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Respawn` (0x07, clientbound, play): the player is moved to another
//! world.
//!
//! Layout (MCP `S07PacketRespawn`, `readPacketData`): `i32` dimension id,
//! `u8` difficulty, `u8` game type, then the world-type string (16-char
//! cap).
//!
//! 1.8 has no hardcore flag and no game-mode bitfield here: the hardcore
//! bit lives in the gamemode byte of `Join Game` (0x01).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_i32, read_u8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// `Respawn` (0x07, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Respawn {
    /// Dimension id: -1 = the Nether, 0 = the Overworld, 1 = the End.
    pub dimension: i32,
    /// Difficulty level (0 peaceful … 3 hard).
    pub difficulty: u8,
    /// Game type: 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: u8,
    /// World generator name, e.g. `flat`.
    pub world_type: String,
}

impl Respawn {
    /// Wire packet id.
    pub const ID: u8 = 0x07;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8, or an over-long world
    /// type string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            dimension: read_i32(reader)?,
            difficulty: read_u8(reader)?,
            game_mode: read_u8(reader)?,
            world_type: read_string(reader, 16)?,
        })
    }
}

impl EncodablePacket for Respawn {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.dimension.to_be_bytes())?;
        writer.write_all(&[self.difficulty, self.game_mode])?;
        write_string(writer, &self.world_type, 16)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respawn_wire_shape() {
        let mut expected = Vec::new();
        expected.extend_from_slice(&0i32.to_be_bytes());
        expected.extend([1, 0]);
        write_string(&mut expected, "flat", 16).unwrap();

        let packet = Respawn {
            dimension: 0,
            difficulty: 1,
            game_mode: 0,
            world_type: "flat".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);

        let decoded = Respawn::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn the_nether_has_dimension_minus_one() {
        let mut expected = Vec::new();
        expected.extend_from_slice(&(-1i32).to_be_bytes());
        expected.extend([2, 1]);
        write_string(&mut expected, "default", 16).unwrap();

        let packet = Respawn::decode(&mut &expected[..]).unwrap();
        assert_eq!(packet.dimension, -1);
        assert_eq!(packet.difficulty, 2);
        assert_eq!(packet.game_mode, 1);
        assert_eq!(packet.world_type, "default");
    }
}
