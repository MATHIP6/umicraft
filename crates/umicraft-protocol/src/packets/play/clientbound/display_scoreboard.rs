// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Display Scoreboard` (0x3D, clientbound, play): choose which objective is
//! drawn in a display slot.
//!
//! Layout (MCP `S3DPacketDisplayScoreboard`, `readPacketData`): the slot as a
//! signed byte (`0` tab list, `1` sidebar, `2` below the name, `3..=18` the
//! sidebar of one team colour) and the objective name (cap 16; empty clears
//! the slot).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// Maximum characters of an objective name.
const NAME_CHARS: usize = 16;

/// `Display Scoreboard` (0x3D, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayScoreboard {
    /// The display slot.
    pub position: i8,
    /// The objective to show, or an empty string to clear the slot.
    pub name: String,
}

impl DisplayScoreboard {
    /// Wire packet id.
    pub const ID: u8 = 0x3D;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an over-long string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            position: read_i8(reader)?,
            name: read_string(reader, NAME_CHARS)?,
        })
    }
}

impl EncodablePacket for DisplayScoreboard {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.position)?;
        write_string(writer, &self.name, NAME_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_scoreboard_round_trips() {
        for (position, name) in [(1, "kills"), (0, ""), (18, "teamboard")] {
            let packet = DisplayScoreboard {
                position,
                name: name.to_string(),
            };
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(DisplayScoreboard::decode(&mut &buffer[..]).unwrap(), packet);
        }
    }

    #[test]
    fn the_slot_comes_first_as_one_byte() {
        let mut buffer = Vec::new();
        DisplayScoreboard {
            position: 1,
            name: "a".to_string(),
        }
        .encode_body(&mut buffer)
        .unwrap();
        assert_eq!(buffer, [1, 1, b'a']);
    }
}
