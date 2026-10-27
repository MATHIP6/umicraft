// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Scoreboard Objective` (0x3B, clientbound, play): creates, removes or
//! renames a scoreboard objective.
//!
//! Layout (MCP `S3BPacketScoreboardObjective`, `readPacketData`): the
//! objective name (up to 16 chars), a signed mode byte (0 create, 1 remove,
//! 2 update) and, for create and update only, the display value (up to 32
//! chars) and the render type string (up to 16 chars: `integer` or
//! `hearts`). The reference client ignores any other mode byte; this
//! decoder reports [`ProtocolError::Invariant`] instead, since the rest of
//! the packet cannot be interpreted. An unknown render type falls back to
//! `integer`, as in the reference client.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

const MAX_NAME_CHARS: usize = 16;
const MAX_VALUE_CHARS: usize = 32;
const MAX_RENDER_CHARS: usize = 16;

/// How an objective's score is drawn in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectiveRenderType {
    /// A plain number.
    Integer,
    /// Hearts.
    Hearts,
}

impl ObjectiveRenderType {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Hearts => "hearts",
        }
    }

    fn from_wire_name(name: &str) -> Self {
        if name == "hearts" {
            Self::Hearts
        } else {
            Self::Integer
        }
    }
}

/// `Scoreboard Objective` (0x3B, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreboardObjective {
    /// Mode 0: creates an objective.
    Create {
        /// Objective name.
        name: String,
        /// Display name.
        value: String,
        /// Score rendering.
        render_type: ObjectiveRenderType,
    },
    /// Mode 1: removes an objective.
    Remove {
        /// Objective name.
        name: String,
    },
    /// Mode 2: changes the display name and rendering.
    Update {
        /// Objective name.
        name: String,
        /// Display name.
        value: String,
        /// Score rendering.
        render_type: ObjectiveRenderType,
    },
}

impl ScoreboardObjective {
    /// Wire packet id.
    pub const ID: u8 = 0x3B;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an invalid string or an unknown mode.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, MAX_NAME_CHARS)?;
        let mode = read_i8(reader)?;
        if mode == 1 {
            return Ok(Self::Remove { name });
        }
        if mode != 0 && mode != 2 {
            return Err(ProtocolError::Invariant(format!(
                "unknown objective mode {mode}"
            )));
        }
        let value = read_string(reader, MAX_VALUE_CHARS)?;
        let render_type =
            ObjectiveRenderType::from_wire_name(&read_string(reader, MAX_RENDER_CHARS)?);
        Ok(if mode == 0 {
            Self::Create {
                name,
                value,
                render_type,
            }
        } else {
            Self::Update {
                name,
                value,
                render_type,
            }
        })
    }
}

fn write_details(
    writer: &mut impl Write,
    name: &str,
    mode: i8,
    value: &str,
    render_type: ObjectiveRenderType,
) -> Result<(), ProtocolError> {
    write_string(writer, name, MAX_NAME_CHARS)?;
    write_i8(writer, mode)?;
    write_string(writer, value, MAX_VALUE_CHARS)?;
    write_string(writer, render_type.wire_name(), MAX_RENDER_CHARS)
}

impl EncodablePacket for ScoreboardObjective {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match self {
            Self::Create {
                name,
                value,
                render_type,
            } => write_details(writer, name, 0, value, *render_type),
            Self::Remove { name } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_i8(writer, 1)
            }
            Self::Update {
                name,
                value,
                render_type,
            } => write_details(writer, name, 2, value, *render_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(packet: ScoreboardObjective, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            ScoreboardObjective::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn scoreboard_objective_wire_shapes() {
        let mut create = vec![1, b'o', 0, 1, b'V', 7];
        create.extend(b"integer");
        check(
            ScoreboardObjective::Create {
                name: "o".to_string(),
                value: "V".to_string(),
                render_type: ObjectiveRenderType::Integer,
            },
            &create,
        );
        check(
            ScoreboardObjective::Remove {
                name: "o".to_string(),
            },
            &[1, b'o', 1],
        );
        let mut update = vec![1, b'o', 2, 1, b'V', 6];
        update.extend(b"hearts");
        check(
            ScoreboardObjective::Update {
                name: "o".to_string(),
                value: "V".to_string(),
                render_type: ObjectiveRenderType::Hearts,
            },
            &update,
        );
    }

    #[test]
    fn an_unknown_render_type_is_integer() {
        let mut bytes = vec![1, b'o', 0, 0, 3];
        bytes.extend(b"xyz");
        let decoded = ScoreboardObjective::decode(&mut &bytes[..]).unwrap();
        assert!(matches!(
            decoded,
            ScoreboardObjective::Create {
                render_type: ObjectiveRenderType::Integer,
                ..
            }
        ));
    }

    #[test]
    fn hostile_scoreboard_objective_is_an_error() {
        assert!(ScoreboardObjective::decode(&mut &[][..]).is_err());
        assert!(ScoreboardObjective::decode(&mut &[1, b'o'][..]).is_err());
        assert!(ScoreboardObjective::decode(&mut &[1, b'o', 3][..]).is_err());
        assert!(ScoreboardObjective::decode(&mut &[1, b'o', 0, 1, b'V'][..]).is_err());
    }
}
