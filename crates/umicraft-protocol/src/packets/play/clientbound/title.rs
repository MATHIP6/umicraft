// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Title` (0x45, clientbound, play): on-screen titles.
//!
//! Layout (MCP `S45PacketTitle`, `readPacketData`): a VarInt action
//! (0 title, 1 subtitle, 2 times, 3 clear, 4 reset), then for title and
//! subtitle a chat JSON string (up to 32767 chars), for times three i32
//! (fade in, stay, fade out, in ticks). The reference client indexes an enum
//! with the action and crashes on an unknown one; here it is
//! [`ProtocolError::Invariant`].

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

const MAX_CHAT_CHARS: usize = 32767;

/// `Title` (0x45, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Title {
    /// Sets and shows the main title.
    Title {
        /// Chat JSON.
        json: String,
    },
    /// Sets the subtitle.
    Subtitle {
        /// Chat JSON.
        json: String,
    },
    /// Sets the display timings, in ticks.
    Times {
        /// Fade-in duration.
        fade_in: i32,
        /// Time the title stays fully visible.
        stay: i32,
        /// Fade-out duration.
        fade_out: i32,
    },
    /// Hides the current title.
    Clear,
    /// Hides the title and restores the default timings.
    Reset,
}

impl Title {
    /// Wire packet id.
    pub const ID: u8 = 0x45;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an invalid string or an unknown action.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        match read_var_i32(reader)? {
            0 => Ok(Self::Title {
                json: read_string(reader, MAX_CHAT_CHARS)?,
            }),
            1 => Ok(Self::Subtitle {
                json: read_string(reader, MAX_CHAT_CHARS)?,
            }),
            2 => Ok(Self::Times {
                fade_in: read_i32(reader)?,
                stay: read_i32(reader)?,
                fade_out: read_i32(reader)?,
            }),
            3 => Ok(Self::Clear),
            4 => Ok(Self::Reset),
            other => Err(ProtocolError::Invariant(format!(
                "unknown title action {other}"
            ))),
        }
    }
}

impl EncodablePacket for Title {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match self {
            Self::Title { json } => {
                write_var_u32(writer, 0)?;
                write_string(writer, json, MAX_CHAT_CHARS)
            }
            Self::Subtitle { json } => {
                write_var_u32(writer, 1)?;
                write_string(writer, json, MAX_CHAT_CHARS)
            }
            Self::Times {
                fade_in,
                stay,
                fade_out,
            } => {
                write_var_u32(writer, 2)?;
                for value in [fade_in, stay, fade_out] {
                    writer.write_all(&value.to_be_bytes())?;
                }
                Ok(())
            }
            Self::Clear => write_var_u32(writer, 3),
            Self::Reset => write_var_u32(writer, 4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(packet: Title, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Title::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn title_wire_shapes() {
        check(
            Title::Title {
                json: "{}".to_string(),
            },
            &[0, 2, b'{', b'}'],
        );
        check(
            Title::Subtitle {
                json: "{}".to_string(),
            },
            &[1, 2, b'{', b'}'],
        );
        check(
            Title::Times {
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            },
            &[2, 0, 0, 0, 10, 0, 0, 0, 70, 0, 0, 0, 20],
        );
        check(Title::Clear, &[3]);
        check(Title::Reset, &[4]);
    }

    #[test]
    fn hostile_title_is_an_error() {
        assert!(Title::decode(&mut &[][..]).is_err());
        assert!(Title::decode(&mut &[5][..]).is_err());
        assert!(Title::decode(&mut &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F][..]).is_err());
        assert!(Title::decode(&mut &[2, 0, 0, 0, 10][..]).is_err());
        assert!(Title::decode(&mut &[0, 2, b'{'][..]).is_err());
    }
}
