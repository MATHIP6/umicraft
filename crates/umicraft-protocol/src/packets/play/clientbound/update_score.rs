// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Score` (0x3C, clientbound, play): sets or removes one score.
//!
//! Layout (MCP `S3CPacketUpdateScore`, `readPacketData`): the score holder
//! name (up to 40 chars), a VarInt action (0 change, 1 remove), the
//! objective name (up to 16 chars) and, for a change only, a VarInt value.
//! The reference client indexes an enum with the action and crashes on an
//! unknown one; here it is [`ProtocolError::Invariant`].

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

const MAX_NAME_CHARS: usize = 40;
const MAX_OBJECTIVE_CHARS: usize = 16;

/// `Update Score` (0x3C, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateScore {
    /// Sets the score of `name` in `objective`.
    Change {
        /// Score holder (player name or entity UUID string).
        name: String,
        /// Objective name.
        objective: String,
        /// New score.
        value: i32,
    },
    /// Removes the score of `name` (from `objective`, or from all objectives
    /// when `objective` is empty).
    Remove {
        /// Score holder.
        name: String,
        /// Objective name; may be empty.
        objective: String,
    },
}

impl UpdateScore {
    /// Wire packet id.
    pub const ID: u8 = 0x3C;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an invalid string or an unknown action.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, MAX_NAME_CHARS)?;
        let action = read_var_i32(reader)?;
        let objective = read_string(reader, MAX_OBJECTIVE_CHARS)?;
        match action {
            0 => Ok(Self::Change {
                name,
                objective,
                value: read_var_i32(reader)?,
            }),
            1 => Ok(Self::Remove { name, objective }),
            other => Err(ProtocolError::Invariant(format!(
                "unknown score action {other}"
            ))),
        }
    }
}

impl EncodablePacket for UpdateScore {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match self {
            Self::Change {
                name,
                objective,
                value,
            } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_var_u32(writer, 0)?;
                write_string(writer, objective, MAX_OBJECTIVE_CHARS)?;
                write_var_u32(writer, *value as u32)
            }
            Self::Remove { name, objective } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_var_u32(writer, 1)?;
                write_string(writer, objective, MAX_OBJECTIVE_CHARS)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(packet: UpdateScore, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(UpdateScore::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn update_score_wire_shapes() {
        check(
            UpdateScore::Change {
                name: "Bob".to_string(),
                objective: "k".to_string(),
                value: 5,
            },
            &[3, b'B', b'o', b'b', 0, 1, b'k', 5],
        );
        check(
            UpdateScore::Remove {
                name: "Bob".to_string(),
                objective: String::new(),
            },
            &[3, b'B', b'o', b'b', 1, 0],
        );
    }

    #[test]
    fn hostile_update_score_is_an_error() {
        assert!(UpdateScore::decode(&mut &[][..]).is_err());
        assert!(UpdateScore::decode(&mut &[1, b'B', 2, 0][..]).is_err());
        // Change without its value.
        assert!(UpdateScore::decode(&mut &[1, b'B', 0, 0][..]).is_err());
    }
}
