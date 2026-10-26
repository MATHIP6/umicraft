// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Title` (0x45, clientbound, play): set or clear the on-screen title.
//!
//! Layout (MCP `S45PacketTitle`, `readPacketData`): a VarInt action ordinal
//! (`TITLE` 0, `SUBTITLE` 1, `TIMES` 2, `CLEAR` 3, `RESET` 4), then for
//! `TITLE` and `SUBTITLE` a chat JSON string, and for `TIMES` three plain
//! big-endian `i32` values (fade in, stay, fade out, in ticks). An ordinal
//! outside `0..=4` makes the reference client throw, so it is an error here.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string, MAX_JSON_CHARS};
use crate::varint::{read_var_i32, write_var_u32};

/// `Title` (0x45, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Title {
    /// Show this chat JSON as the title.
    Title(String),
    /// Show this chat JSON as the subtitle.
    Subtitle(String),
    /// Set the display timings, in ticks.
    Times {
        /// Fade-in duration.
        fade_in: i32,
        /// Time the title stays fully visible.
        stay: i32,
        /// Fade-out duration.
        fade_out: i32,
    },
    /// Hide the current title.
    Clear,
    /// Hide the title and restore the default timings.
    Reset,
}

impl Title {
    /// Wire packet id.
    pub const ID: u8 = 0x45;

    /// The action ordinal on the wire.
    const fn ordinal(&self) -> u32 {
        match self {
            Self::Title(_) => 0,
            Self::Subtitle(_) => 1,
            Self::Times { .. } => 2,
            Self::Clear => 3,
            Self::Reset => 4,
        }
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an unknown action or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match read_var_i32(reader)? {
            0 => Self::Title(read_string(reader, MAX_JSON_CHARS)?),
            1 => Self::Subtitle(read_string(reader, MAX_JSON_CHARS)?),
            2 => Self::Times {
                fade_in: read_i32(reader)?,
                stay: read_i32(reader)?,
                fade_out: read_i32(reader)?,
            },
            3 => Self::Clear,
            4 => Self::Reset,
            other => {
                return Err(ProtocolError::Invariant(format!(
                    "unknown title action {other}"
                )))
            }
        })
    }
}

impl EncodablePacket for Title {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.ordinal())?;
        match self {
            Self::Title(json) | Self::Subtitle(json) => write_string(writer, json, MAX_JSON_CHARS),
            Self::Times {
                fade_in,
                stay,
                fade_out,
            } => {
                writer.write_all(&fade_in.to_be_bytes())?;
                writer.write_all(&stay.to_be_bytes())?;
                writer.write_all(&fade_out.to_be_bytes())?;
                Ok(())
            }
            Self::Clear | Self::Reset => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &Title) -> Title {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        Title::decode(&mut &buffer[..]).unwrap()
    }

    #[test]
    fn every_action_round_trips() {
        for packet in [
            Title::Title(r#"{"text":"Hi"}"#.to_string()),
            Title::Subtitle(r#"{"text":"Sub"}"#.to_string()),
            Title::Times {
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            },
            Title::Clear,
            Title::Reset,
        ] {
            assert_eq!(round_trip(&packet), packet);
        }
    }

    #[test]
    fn times_use_plain_big_endian_ints_after_the_action() {
        let mut buffer = Vec::new();
        Title::Times {
            fade_in: 1,
            stay: 2,
            fade_out: 3,
        }
        .encode_body(&mut buffer)
        .unwrap();
        assert_eq!(buffer, [2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3]);
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        assert!(Title::decode(&mut &[9u8][..]).is_err());
    }
}
