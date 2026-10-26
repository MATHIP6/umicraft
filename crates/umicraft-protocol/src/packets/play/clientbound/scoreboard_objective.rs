// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Scoreboard Objective` (0x3B, clientbound, play): create, remove or update
//! a scoreboard objective.
//!
//! Layout (MCP `S3BPacketScoreboardObjective`, `readPacketData`): the
//! objective name (cap 16), a mode byte (`0` create, `1` remove, `2` update),
//! and, only for modes `0` and `2`, the display name (cap 32) and the render
//! type string (cap 16, `"integer"` or `"hearts"`). The reference client
//! ignores any other mode byte instead of failing, so the mode is kept raw.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// The mode byte that creates an objective.
pub const MODE_CREATE: i8 = 0;
/// The mode byte that removes an objective.
pub const MODE_REMOVE: i8 = 1;
/// The mode byte that updates the display of an objective.
pub const MODE_UPDATE: i8 = 2;

/// Maximum characters of an objective name.
const NAME_CHARS: usize = 16;
/// Maximum characters of an objective display name.
const DISPLAY_NAME_CHARS: usize = 32;
/// Maximum characters of a render type.
const RENDER_TYPE_CHARS: usize = 16;

/// The display fields that only the create and update modes carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveInfo {
    /// The name shown to the player.
    pub display_name: String,
    /// How the score is drawn: `"integer"` or `"hearts"`.
    pub render_type: String,
}

/// `Scoreboard Objective` (0x3B, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreboardObjective {
    /// The internal objective name.
    pub name: String,
    /// The raw mode byte; see [`MODE_CREATE`], [`MODE_REMOVE`], [`MODE_UPDATE`].
    pub mode: i8,
    /// The display fields, present exactly for the create and update modes.
    pub info: Option<ObjectiveInfo>,
}

impl ScoreboardObjective {
    /// Wire packet id.
    pub const ID: u8 = 0x3B;

    /// Whether `mode` carries the display fields.
    #[must_use]
    pub const fn mode_has_info(mode: i8) -> bool {
        mode == MODE_CREATE || mode == MODE_UPDATE
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an over-long string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, NAME_CHARS)?;
        let mode = read_i8(reader)?;
        let info = if Self::mode_has_info(mode) {
            Some(ObjectiveInfo {
                display_name: read_string(reader, DISPLAY_NAME_CHARS)?,
                render_type: read_string(reader, RENDER_TYPE_CHARS)?,
            })
        } else {
            None
        };
        Ok(Self { name, mode, info })
    }
}

impl EncodablePacket for ScoreboardObjective {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.name, NAME_CHARS)?;
        write_i8(writer, self.mode)?;
        if Self::mode_has_info(self.mode) {
            let info = self.info.as_ref().ok_or_else(|| {
                ProtocolError::Invariant("objective mode needs its display fields".to_string())
            })?;
            write_string(writer, &info.display_name, DISPLAY_NAME_CHARS)?;
            write_string(writer, &info.render_type, RENDER_TYPE_CHARS)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &ScoreboardObjective) -> ScoreboardObjective {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        ScoreboardObjective::decode(&mut &buffer[..]).unwrap()
    }

    #[test]
    fn create_carries_the_display_fields() {
        let packet = ScoreboardObjective {
            name: "kills".to_string(),
            mode: MODE_CREATE,
            info: Some(ObjectiveInfo {
                display_name: "Kills".to_string(),
                render_type: "integer".to_string(),
            }),
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn remove_carries_nothing_after_the_mode() {
        let packet = ScoreboardObjective {
            name: "kills".to_string(),
            mode: MODE_REMOVE,
            info: None,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer.len(), 1 + 5 + 1);
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn an_unknown_mode_is_kept_raw_like_the_reference_client() {
        let packet = ScoreboardObjective {
            name: "x".to_string(),
            mode: 9,
            info: None,
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn a_name_over_sixteen_chars_is_rejected_on_write() {
        let packet = ScoreboardObjective {
            name: "a".repeat(17),
            mode: MODE_REMOVE,
            info: None,
        };
        assert!(packet.encode_body(&mut Vec::new()).is_err());
    }
}
