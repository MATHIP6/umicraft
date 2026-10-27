// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Display Scoreboard` (0x3D, clientbound, play): picks the objective shown
//! in a scoreboard slot.
//!
//! Layout (MCP `S3DPacketDisplayScoreboard`, `readPacketData`): a signed byte
//! slot (0 list, 1 sidebar, 2 below name, 3-18 team sidebars) and the
//! objective name (up to 16 chars; empty clears the slot).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

const MAX_OBJECTIVE_CHARS: usize = 16;

/// `Display Scoreboard` (0x3D, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayScoreboard {
    /// Display slot.
    pub position: i8,
    /// Objective name; empty removes the objective from the slot.
    pub score_name: String,
}

impl DisplayScoreboard {
    /// Wire packet id.
    pub const ID: u8 = 0x3D;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            position: read_i8(reader)?,
            score_name: read_string(reader, MAX_OBJECTIVE_CHARS)?,
        })
    }
}

impl EncodablePacket for DisplayScoreboard {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.position)?;
        write_string(writer, &self.score_name, MAX_OBJECTIVE_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_scoreboard_wire_shape() {
        let packet = DisplayScoreboard {
            position: 1,
            score_name: "kills".to_string(),
        };
        let expected = [1, 5, b'k', b'i', b'l', b'l', b's'];
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            DisplayScoreboard::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn hostile_display_scoreboard_is_an_error() {
        assert!(DisplayScoreboard::decode(&mut &[][..]).is_err());
        assert!(DisplayScoreboard::decode(&mut &[1, 5, b'k'][..]).is_err());
        let mut long = vec![1, 100];
        long.extend([b'x'; 100]);
        assert!(DisplayScoreboard::decode(&mut &long[..]).is_err());
    }
}
