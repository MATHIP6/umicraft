// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Join Game` (0x01, clientbound, play): entry into the world.
//!
//! Layout pinned to MCP `S01PacketJoinGame`: plain big-endian Int entity id
//! (not a VarInt), gamemode byte with the hardcore bit `0x8` folded in,
//! signed byte dimension, u8 difficulty, u8 player cap, 16-char world-type
//! string, and the 1.8 trailing `reducedDebugInfo` bool.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_i32, read_u8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// `Join Game` (0x01, clientbound, play): first play-state packet after
/// `Login Success`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinGame {
    /// The player's entity id (plain 4-byte Int in 1.8).
    pub entity_id: i32,
    /// Raw gamemode byte (hardcore bit 8 folded in by 1.8).
    pub gamemode: u8,
    /// Dimension id (0 overworld; 1.8 sends it as a signed byte).
    pub dimension: i8,
    /// Difficulty level byte.
    pub difficulty: u8,
    /// Player cap advertised by the server.
    pub max_players: u8,
    /// World generator name, e.g. `flat` on test worlds.
    pub level_type: String,
    /// 1.8 trailing flag disabling the debug F3 pie chart overlay.
    pub reduced_debug_info: bool,
}

impl JoinGame {
    /// Wire packet id.
    pub const ID: u8 = 0x01;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8, or an over-long world
    /// type string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_i32(reader)?,
            gamemode: read_u8(reader)?,
            dimension: read_u8(reader)? as i8,
            difficulty: read_u8(reader)?,
            max_players: read_u8(reader)?,
            level_type: read_string(reader, 16)?,
            reduced_debug_info: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for JoinGame {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        // Entity id is a plain big-endian Int in 1.8 (not a VarInt).
        writer.write_all(&self.entity_id.to_be_bytes())?;
        writer.write_all(&[
            self.gamemode,
            self.dimension as u8,
            self.difficulty,
            self.max_players,
        ])?;
        write_string(writer, &self.level_type, 16)?;
        writer.write_all(&[u8::from(self.reduced_debug_info)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_game_wire_shape_matches_1_8() {
        // Int entity id, gamemode byte (hardcore bit 8), byte dimension, u8
        // difficulty, u8 cap, 16-cap world string, trailing reduced-debug
        // bool.
        let packet = JoinGame {
            entity_id: 26,
            gamemode: 0,
            dimension: 0,
            difficulty: 1,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        };
        let mut expected = Vec::new();
        expected.extend(26i32.to_be_bytes());
        expected.extend([0, 0, 1, 20]);
        write_string(&mut expected, "flat", 16).unwrap();
        expected.push(0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(JoinGame::decode(&mut &expected[..]).unwrap(), packet);

        // Wide and negative values survive the fixed-width Int read.
        let packet = JoinGame {
            entity_id: -123_456_789,
            gamemode: 1,
            dimension: -1,
            difficulty: 2,
            max_players: 5,
            level_type: "default".to_string(),
            reduced_debug_info: true,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(JoinGame::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
