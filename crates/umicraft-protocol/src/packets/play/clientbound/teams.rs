// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Teams` (0x3E, clientbound, play): scoreboard team management.
//!
//! Layout (MCP `S3EPacketTeams`, `readPacketData`): the team name (up to 16
//! chars) and a signed mode byte: 0 create, 1 remove, 2 update info, 3 add
//! players, 4 remove players. Modes 0 and 2 carry the team info: display
//! name (32 chars), prefix (16), suffix (16), a friendly-flags byte
//! (bit 0 friendly fire, bit 1 see invisible teammates), the name-tag
//! visibility string (32) and a colour byte (-1 = none). Modes 0, 3 and 4
//! then carry a VarInt count and that many player names (up to 40 chars).
//! The reference client ignores any other mode byte; this decoder reports
//! [`ProtocolError::Invariant`] instead.

use std::io::{Read, Write};

use super::statistics::read_list_len;
use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::write_var_u32;

const MAX_NAME_CHARS: usize = 16;
const MAX_DISPLAY_CHARS: usize = 32;
const MAX_AFFIX_CHARS: usize = 16;
const MAX_VISIBILITY_CHARS: usize = 32;
const MAX_PLAYER_CHARS: usize = 40;

/// Display and behaviour settings of a team (modes 0 and 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamInfo {
    /// Display name.
    pub display_name: String,
    /// Text put before member names.
    pub prefix: String,
    /// Text put after member names.
    pub suffix: String,
    /// Bit 0 friendly fire, bit 1 see invisible teammates.
    pub friendly_flags: i8,
    /// Name-tag visibility rule: `always`, `never`, `hideForOtherTeams` or
    /// `hideForOwnTeam`.
    pub name_tag_visibility: String,
    /// Chat colour index of the team, -1 for none.
    pub color: i8,
}

impl TeamInfo {
    fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            display_name: read_string(reader, MAX_DISPLAY_CHARS)?,
            prefix: read_string(reader, MAX_AFFIX_CHARS)?,
            suffix: read_string(reader, MAX_AFFIX_CHARS)?,
            friendly_flags: read_i8(reader)?,
            name_tag_visibility: read_string(reader, MAX_VISIBILITY_CHARS)?,
            color: read_i8(reader)?,
        })
    }

    fn encode(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.display_name, MAX_DISPLAY_CHARS)?;
        write_string(writer, &self.prefix, MAX_AFFIX_CHARS)?;
        write_string(writer, &self.suffix, MAX_AFFIX_CHARS)?;
        write_i8(writer, self.friendly_flags)?;
        write_string(writer, &self.name_tag_visibility, MAX_VISIBILITY_CHARS)?;
        write_i8(writer, self.color)
    }
}

fn read_players(reader: &mut impl Read) -> Result<Vec<String>, ProtocolError> {
    let len = read_list_len(reader)?;
    let mut players = Vec::new();
    for _ in 0..len {
        players.push(read_string(reader, MAX_PLAYER_CHARS)?);
    }
    Ok(players)
}

fn write_players(writer: &mut impl Write, players: &[String]) -> Result<(), ProtocolError> {
    write_var_u32(writer, players.len() as u32)?;
    for player in players {
        write_string(writer, player, MAX_PLAYER_CHARS)?;
    }
    Ok(())
}

/// `Teams` (0x3E, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Teams {
    /// Mode 0: creates a team with its first members.
    Create {
        /// Team name.
        name: String,
        /// Team settings.
        info: TeamInfo,
        /// Initial members.
        players: Vec<String>,
    },
    /// Mode 1: removes a team.
    Remove {
        /// Team name.
        name: String,
    },
    /// Mode 2: changes the team settings.
    Update {
        /// Team name.
        name: String,
        /// New settings.
        info: TeamInfo,
    },
    /// Mode 3: adds members.
    AddPlayers {
        /// Team name.
        name: String,
        /// Members to add.
        players: Vec<String>,
    },
    /// Mode 4: removes members.
    RemovePlayers {
        /// Team name.
        name: String,
        /// Members to remove.
        players: Vec<String>,
    },
}

impl Teams {
    /// Wire packet id.
    pub const ID: u8 = 0x3E;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a bad count, an invalid string or an
    /// unknown mode.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, MAX_NAME_CHARS)?;
        match read_i8(reader)? {
            0 => {
                let info = TeamInfo::decode(reader)?;
                let players = read_players(reader)?;
                Ok(Self::Create {
                    name,
                    info,
                    players,
                })
            }
            1 => Ok(Self::Remove { name }),
            2 => Ok(Self::Update {
                name,
                info: TeamInfo::decode(reader)?,
            }),
            3 => Ok(Self::AddPlayers {
                name,
                players: read_players(reader)?,
            }),
            4 => Ok(Self::RemovePlayers {
                name,
                players: read_players(reader)?,
            }),
            other => Err(ProtocolError::Invariant(format!(
                "unknown team mode {other}"
            ))),
        }
    }
}

impl EncodablePacket for Teams {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match self {
            Self::Create {
                name,
                info,
                players,
            } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_i8(writer, 0)?;
                info.encode(writer)?;
                write_players(writer, players)
            }
            Self::Remove { name } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_i8(writer, 1)
            }
            Self::Update { name, info } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_i8(writer, 2)?;
                info.encode(writer)
            }
            Self::AddPlayers { name, players } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_i8(writer, 3)?;
                write_players(writer, players)
            }
            Self::RemovePlayers { name, players } => {
                write_string(writer, name, MAX_NAME_CHARS)?;
                write_i8(writer, 4)?;
                write_players(writer, players)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> TeamInfo {
        TeamInfo {
            display_name: "D".to_string(),
            prefix: "P".to_string(),
            suffix: "S".to_string(),
            friendly_flags: 3,
            name_tag_visibility: "always".to_string(),
            color: -1,
        }
    }

    fn info_bytes() -> Vec<u8> {
        let mut bytes = vec![1, b'D', 1, b'P', 1, b'S', 3, 6];
        bytes.extend(b"always");
        bytes.push(0xFF);
        bytes
    }

    fn check(packet: Teams, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Teams::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn teams_wire_shapes() {
        let mut create = vec![1, b't', 0];
        create.extend(info_bytes());
        create.extend([1, 1, b'a']);
        check(
            Teams::Create {
                name: "t".to_string(),
                info: info(),
                players: vec!["a".to_string()],
            },
            &create,
        );
        check(
            Teams::Remove {
                name: "t".to_string(),
            },
            &[1, b't', 1],
        );
        let mut update = vec![1, b't', 2];
        update.extend(info_bytes());
        check(
            Teams::Update {
                name: "t".to_string(),
                info: info(),
            },
            &update,
        );
        check(
            Teams::AddPlayers {
                name: "t".to_string(),
                players: vec!["a".to_string(), "b".to_string()],
            },
            &[1, b't', 3, 2, 1, b'a', 1, b'b'],
        );
        check(
            Teams::RemovePlayers {
                name: "t".to_string(),
                players: vec![],
            },
            &[1, b't', 4, 0],
        );
    }

    #[test]
    fn hostile_teams_are_errors() {
        assert!(Teams::decode(&mut &[][..]).is_err());
        assert!(Teams::decode(&mut &[1, b't'][..]).is_err());
        assert!(Teams::decode(&mut &[1, b't', 5][..]).is_err());
        assert!(Teams::decode(&mut &[1, b't', 0, 1, b'D'][..]).is_err());
        assert!(Teams::decode(&mut &[1, b't', 3, 2, 1, b'a'][..]).is_err());
        assert!(Teams::decode(&mut &[1, b't', 3, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F][..]).is_err());
    }
}
