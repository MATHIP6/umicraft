// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Teams` (0x3E, clientbound, play): create, update or remove a scoreboard
//! team and change its members.
//!
//! Layout (MCP `S3EPacketTeams`, `readPacketData`): the team name (cap 16), a
//! mode byte, then:
//!
//! - modes `0` (create) and `2` (update): display name (cap 32), prefix
//!   (cap 16), suffix (cap 16), friendly-fire flags byte, name-tag
//!   visibility string (cap 32), colour byte;
//! - modes `0`, `3` (add players) and `4` (remove players): a VarInt member
//!   count and that many member names (cap 40).
//!
//! Mode `1` removes the team. The reference client ignores any other mode
//! byte, so the mode is kept raw.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

/// The mode byte that creates a team.
pub const MODE_CREATE: i8 = 0;
/// The mode byte that removes a team.
pub const MODE_REMOVE: i8 = 1;
/// The mode byte that updates the display of a team.
pub const MODE_UPDATE: i8 = 2;
/// The mode byte that adds members to a team.
pub const MODE_ADD_PLAYERS: i8 = 3;
/// The mode byte that removes members from a team.
pub const MODE_REMOVE_PLAYERS: i8 = 4;

/// Maximum number of members accepted in one packet.
///
/// Every name costs at least one byte and a whole packet is capped at 2 MiB,
/// so a bigger count can only come from a hostile server.
const MAX_MEMBERS: usize = 4096;
/// Maximum characters of a team name.
const NAME_CHARS: usize = 16;
/// Maximum characters of a team display name or name-tag visibility.
const DISPLAY_CHARS: usize = 32;
/// Maximum characters of a team prefix or suffix.
const AFFIX_CHARS: usize = 16;
/// Maximum characters of a member name.
const MEMBER_CHARS: usize = 40;

/// The display fields that only the create and update modes carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamInfo {
    /// The team display name.
    pub display_name: String,
    /// Text drawn before the member names.
    pub prefix: String,
    /// Text drawn after the member names.
    pub suffix: String,
    /// Bit 0: friendly fire; bit 1: see invisible team mates.
    pub friendly_flags: i8,
    /// When name tags are visible: `always`, `never`, `hideForOtherTeams`, …
    pub name_tag_visibility: String,
    /// The team colour as a chat formatting index, `-1` for none.
    pub color: i8,
}

/// `Teams` (0x3E, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Teams {
    /// The team name.
    pub name: String,
    /// The raw mode byte; see the `MODE_*` constants.
    pub mode: i8,
    /// The display fields, present exactly for the create and update modes.
    pub info: Option<TeamInfo>,
    /// The members, present for the create, add and remove-players modes.
    pub players: Vec<String>,
}

impl Teams {
    /// Wire packet id.
    pub const ID: u8 = 0x3E;

    /// Whether `mode` carries the display fields.
    #[must_use]
    pub const fn mode_has_info(mode: i8) -> bool {
        mode == MODE_CREATE || mode == MODE_UPDATE
    }

    /// Whether `mode` carries a member list.
    #[must_use]
    pub const fn mode_has_players(mode: i8) -> bool {
        mode == MODE_CREATE || mode == MODE_ADD_PLAYERS || mode == MODE_REMOVE_PLAYERS
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an over-long string or a member count
    /// that is negative or above the budget.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, NAME_CHARS)?;
        let mode = read_i8(reader)?;
        let info = if Self::mode_has_info(mode) {
            Some(TeamInfo {
                display_name: read_string(reader, DISPLAY_CHARS)?,
                prefix: read_string(reader, AFFIX_CHARS)?,
                suffix: read_string(reader, AFFIX_CHARS)?,
                friendly_flags: read_i8(reader)?,
                name_tag_visibility: read_string(reader, DISPLAY_CHARS)?,
                color: read_i8(reader)?,
            })
        } else {
            None
        };
        let mut players = Vec::new();
        if Self::mode_has_players(mode) {
            let count = read_var_i32(reader)?;
            let count = usize::try_from(count)
                .ok()
                .filter(|count| *count <= MAX_MEMBERS)
                .ok_or_else(|| {
                    ProtocolError::Invariant(format!("team member count {count} is out of range"))
                })?;
            players.reserve(count);
            for _ in 0..count {
                players.push(read_string(reader, MEMBER_CHARS)?);
            }
        }
        Ok(Self {
            name,
            mode,
            info,
            players,
        })
    }
}

impl EncodablePacket for Teams {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.name, NAME_CHARS)?;
        write_i8(writer, self.mode)?;
        if Self::mode_has_info(self.mode) {
            let info = self.info.as_ref().ok_or_else(|| {
                ProtocolError::Invariant("team mode needs its display fields".to_string())
            })?;
            write_string(writer, &info.display_name, DISPLAY_CHARS)?;
            write_string(writer, &info.prefix, AFFIX_CHARS)?;
            write_string(writer, &info.suffix, AFFIX_CHARS)?;
            write_i8(writer, info.friendly_flags)?;
            write_string(writer, &info.name_tag_visibility, DISPLAY_CHARS)?;
            write_i8(writer, info.color)?;
        }
        if Self::mode_has_players(self.mode) {
            write_var_u32(writer, self.players.len() as u32)?;
            for player in &self.players {
                write_string(writer, player, MEMBER_CHARS)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> TeamInfo {
        TeamInfo {
            display_name: "Red".to_string(),
            prefix: "[R] ".to_string(),
            suffix: String::new(),
            friendly_flags: 3,
            name_tag_visibility: "always".to_string(),
            color: 12,
        }
    }

    fn round_trip(packet: &Teams) -> Teams {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        Teams::decode(&mut &buffer[..]).unwrap()
    }

    #[test]
    fn create_carries_the_display_fields_and_the_members() {
        let packet = Teams {
            name: "red".to_string(),
            mode: MODE_CREATE,
            info: Some(info()),
            players: vec!["Steve".to_string(), "Alex".to_string()],
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn update_has_the_display_fields_but_no_members() {
        let packet = Teams {
            name: "red".to_string(),
            mode: MODE_UPDATE,
            info: Some(info()),
            players: vec![],
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn add_and_remove_players_have_members_but_no_display_fields() {
        for mode in [MODE_ADD_PLAYERS, MODE_REMOVE_PLAYERS] {
            let packet = Teams {
                name: "red".to_string(),
                mode,
                info: None,
                players: vec!["Steve".to_string()],
            };
            assert_eq!(round_trip(&packet), packet);
        }
    }

    #[test]
    fn remove_and_unknown_modes_carry_nothing() {
        for mode in [MODE_REMOVE, 9] {
            let packet = Teams {
                name: "red".to_string(),
                mode,
                info: None,
                players: vec![],
            };
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(buffer.len(), 1 + 3 + 1);
            assert_eq!(round_trip(&packet), packet);
        }
    }

    #[test]
    fn an_oversized_member_count_is_rejected_before_allocation() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "t", NAME_CHARS).unwrap();
        buffer.push(MODE_ADD_PLAYERS as u8);
        buffer.extend([0x88, 0x27]); // VarInt 5000
        assert!(Teams::decode(&mut &buffer[..]).is_err());
    }
}
