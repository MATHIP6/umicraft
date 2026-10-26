// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Score` (0x3C, clientbound, play): set or remove one score line.
//!
//! Layout (MCP `S3CPacketUpdateScore`, `readPacketData`): the score holder
//! name (cap 40), a VarInt action (`0` change, `1` remove), the objective
//! name (cap 16), and, unless the action removes the score, the value as a
//! VarInt. An action ordinal other than `0` or `1` makes the reference client
//! throw, so it is an error here.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

/// Maximum characters of a score holder name.
const HOLDER_CHARS: usize = 40;
/// Maximum characters of an objective name.
const OBJECTIVE_CHARS: usize = 16;

/// `Update Score` (0x3C, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateScore {
    /// The score holder: a player name or an arbitrary string.
    pub name: String,
    /// The objective the score belongs to.
    pub objective: String,
    /// The new value, or `None` when the score is removed.
    pub value: Option<i32>,
}

impl UpdateScore {
    /// Wire packet id.
    pub const ID: u8 = 0x3C;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an unknown action or an over-long
    /// string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, HOLDER_CHARS)?;
        let remove = match read_var_i32(reader)? {
            0 => false,
            1 => true,
            other => {
                return Err(ProtocolError::Invariant(format!(
                    "unknown score action {other}"
                )))
            }
        };
        let objective = read_string(reader, OBJECTIVE_CHARS)?;
        let value = if remove {
            None
        } else {
            Some(read_var_i32(reader)?)
        };
        Ok(Self {
            name,
            objective,
            value,
        })
    }
}

impl EncodablePacket for UpdateScore {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.name, HOLDER_CHARS)?;
        write_var_u32(writer, u32::from(self.value.is_none()))?;
        write_string(writer, &self.objective, OBJECTIVE_CHARS)?;
        if let Some(value) = self.value {
            write_var_u32(writer, value as u32)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &UpdateScore) -> UpdateScore {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        UpdateScore::decode(&mut &buffer[..]).unwrap()
    }

    #[test]
    fn a_change_carries_its_value_including_negative_ones() {
        for value in [0, 7, -3, i32::MAX, i32::MIN] {
            let packet = UpdateScore {
                name: "Steve".to_string(),
                objective: "kills".to_string(),
                value: Some(value),
            };
            assert_eq!(round_trip(&packet), packet);
        }
    }

    #[test]
    fn a_removal_has_no_value() {
        let packet = UpdateScore {
            name: "Steve".to_string(),
            objective: "kills".to_string(),
            value: None,
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "x", HOLDER_CHARS).unwrap();
        buffer.push(2);
        assert!(UpdateScore::decode(&mut &buffer[..]).is_err());
    }
}
