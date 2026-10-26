// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player List Item` (0x38, clientbound, play): add, update or remove
//! entries of the tab list.
//!
//! Layout (MCP `S38PacketPlayerListItem`, `readPacketData`): a VarInt action
//! (`ADD_PLAYER` 0, `UPDATE_GAME_MODE` 1, `UPDATE_LATENCY` 2,
//! `UPDATE_DISPLAY_NAME` 3, `REMOVE_PLAYER` 4), a VarInt entry count, then per
//! entry the 16-byte UUID followed by fields that depend on the action:
//!
//! | action | fields after the UUID |
//! |---|---|
//! | add | name (cap 16), VarInt property count, per property name, value, bool, optional signature (each cap 32767), VarInt game mode, VarInt ping, bool, optional display name (chat JSON) |
//! | game mode | VarInt game mode |
//! | latency | VarInt ping in milliseconds |
//! | display name | bool, optional display name (chat JSON) |
//! | remove | nothing |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_bool;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string, MAX_JSON_CHARS};
use crate::varint::{read_var_i32, write_var_u32};

/// A player UUID, most significant bits first.
pub type Uuid = [u8; 16];

/// Maximum number of entries accepted in one packet.
///
/// Every entry costs at least 17 bytes and a whole packet is capped at
/// 2 MiB, so a bigger count can only come from a hostile server.
const MAX_ENTRIES: usize = 4096;
/// Maximum number of profile properties accepted for one entry.
const MAX_PROPERTIES: usize = 64;
/// Maximum characters of a player name.
const NAME_CHARS: usize = 16;

/// One profile property of an added player (the skin textures live here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerProperty {
    /// The property name, for example `textures`.
    pub name: String,
    /// The property value, for example the base64 texture payload.
    pub value: String,
    /// The Mojang signature, when the property is signed.
    pub signature: Option<String>,
}

/// An added tab-list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerListAdd {
    /// The player UUID.
    pub uuid: Uuid,
    /// The player name.
    pub name: String,
    /// The profile properties.
    pub properties: Vec<PlayerProperty>,
    /// The game mode id (0 survival, 1 creative, 2 adventure, 3 spectator).
    pub game_mode: i32,
    /// The latency in milliseconds.
    pub ping: i32,
    /// The display name as chat JSON, when the server overrides the name.
    pub display_name: Option<String>,
}

/// `Player List Item` (0x38, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerListItem {
    /// Entries were added.
    Add(Vec<PlayerListAdd>),
    /// The game mode of entries changed.
    UpdateGameMode(Vec<(Uuid, i32)>),
    /// The latency of entries changed.
    UpdateLatency(Vec<(Uuid, i32)>),
    /// The display name of entries changed; `None` restores the plain name.
    UpdateDisplayName(Vec<(Uuid, Option<String>)>),
    /// Entries were removed.
    Remove(Vec<Uuid>),
}

fn read_uuid(reader: &mut impl Read) -> Result<Uuid, ProtocolError> {
    let mut uuid = [0u8; 16];
    reader.read_exact(&mut uuid)?;
    Ok(uuid)
}

fn read_optional_json(reader: &mut impl Read) -> Result<Option<String>, ProtocolError> {
    if read_bool(reader)? {
        Ok(Some(read_string(reader, MAX_JSON_CHARS)?))
    } else {
        Ok(None)
    }
}

fn write_optional_json(writer: &mut impl Write, value: Option<&str>) -> Result<(), ProtocolError> {
    match value {
        Some(json) => {
            writer.write_all(&[1])?;
            write_string(writer, json, MAX_JSON_CHARS)
        }
        None => {
            writer.write_all(&[0])?;
            Ok(())
        }
    }
}

fn read_count(reader: &mut impl Read, limit: usize, what: &str) -> Result<usize, ProtocolError> {
    let count = read_var_i32(reader)?;
    usize::try_from(count)
        .ok()
        .filter(|count| *count <= limit)
        .ok_or_else(|| ProtocolError::Invariant(format!("{what} count {count} is out of range")))
}

fn read_add(reader: &mut impl Read) -> Result<PlayerListAdd, ProtocolError> {
    let uuid = read_uuid(reader)?;
    let name = read_string(reader, NAME_CHARS)?;
    let property_count = read_count(reader, MAX_PROPERTIES, "profile property")?;
    let mut properties = Vec::with_capacity(property_count);
    for _ in 0..property_count {
        let name = read_string(reader, MAX_JSON_CHARS)?;
        let value = read_string(reader, MAX_JSON_CHARS)?;
        let signature = if read_bool(reader)? {
            Some(read_string(reader, MAX_JSON_CHARS)?)
        } else {
            None
        };
        properties.push(PlayerProperty {
            name,
            value,
            signature,
        });
    }
    Ok(PlayerListAdd {
        uuid,
        name,
        properties,
        game_mode: read_var_i32(reader)?,
        ping: read_var_i32(reader)?,
        display_name: read_optional_json(reader)?,
    })
}

impl PlayerListItem {
    /// Wire packet id.
    pub const ID: u8 = 0x38;

    /// The action ordinal on the wire.
    const fn ordinal(&self) -> u32 {
        match self {
            Self::Add(_) => 0,
            Self::UpdateGameMode(_) => 1,
            Self::UpdateLatency(_) => 2,
            Self::UpdateDisplayName(_) => 3,
            Self::Remove(_) => 4,
        }
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an unknown action, an over-long string
    /// or a count that is negative or above the budget.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let action = read_var_i32(reader)?;
        let count = read_count(reader, MAX_ENTRIES, "player list entry")?;
        Ok(match action {
            0 => Self::Add(
                (0..count)
                    .map(|_| read_add(reader))
                    .collect::<Result<_, _>>()?,
            ),
            1 => Self::UpdateGameMode(
                (0..count)
                    .map(|_| Ok((read_uuid(reader)?, read_var_i32(reader)?)))
                    .collect::<Result<_, ProtocolError>>()?,
            ),
            2 => Self::UpdateLatency(
                (0..count)
                    .map(|_| Ok((read_uuid(reader)?, read_var_i32(reader)?)))
                    .collect::<Result<_, ProtocolError>>()?,
            ),
            3 => Self::UpdateDisplayName(
                (0..count)
                    .map(|_| Ok((read_uuid(reader)?, read_optional_json(reader)?)))
                    .collect::<Result<_, ProtocolError>>()?,
            ),
            4 => Self::Remove(
                (0..count)
                    .map(|_| read_uuid(reader))
                    .collect::<Result<_, _>>()?,
            ),
            other => {
                return Err(ProtocolError::Invariant(format!(
                    "unknown player list action {other}"
                )))
            }
        })
    }
}

impl EncodablePacket for PlayerListItem {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.ordinal())?;
        match self {
            Self::Add(entries) => {
                write_var_u32(writer, entries.len() as u32)?;
                for entry in entries {
                    writer.write_all(&entry.uuid)?;
                    write_string(writer, &entry.name, NAME_CHARS)?;
                    write_var_u32(writer, entry.properties.len() as u32)?;
                    for property in &entry.properties {
                        write_string(writer, &property.name, MAX_JSON_CHARS)?;
                        write_string(writer, &property.value, MAX_JSON_CHARS)?;
                        match &property.signature {
                            Some(signature) => {
                                writer.write_all(&[1])?;
                                write_string(writer, signature, MAX_JSON_CHARS)?;
                            }
                            None => writer.write_all(&[0])?,
                        }
                    }
                    write_var_u32(writer, entry.game_mode as u32)?;
                    write_var_u32(writer, entry.ping as u32)?;
                    write_optional_json(writer, entry.display_name.as_deref())?;
                }
            }
            Self::UpdateGameMode(entries) | Self::UpdateLatency(entries) => {
                write_var_u32(writer, entries.len() as u32)?;
                for (uuid, value) in entries {
                    writer.write_all(uuid)?;
                    write_var_u32(writer, *value as u32)?;
                }
            }
            Self::UpdateDisplayName(entries) => {
                write_var_u32(writer, entries.len() as u32)?;
                for (uuid, name) in entries {
                    writer.write_all(uuid)?;
                    write_optional_json(writer, name.as_deref())?;
                }
            }
            Self::Remove(uuids) => {
                write_var_u32(writer, uuids.len() as u32)?;
                for uuid in uuids {
                    writer.write_all(uuid)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &PlayerListItem) -> PlayerListItem {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        PlayerListItem::decode(&mut &buffer[..]).unwrap()
    }

    fn uuid(seed: u8) -> Uuid {
        [seed; 16]
    }

    #[test]
    fn add_round_trips_with_signed_and_unsigned_properties() {
        let packet = PlayerListItem::Add(vec![
            PlayerListAdd {
                uuid: uuid(1),
                name: "Steve".to_string(),
                properties: vec![
                    PlayerProperty {
                        name: "textures".to_string(),
                        value: "eyJ0".to_string(),
                        signature: Some("sig".to_string()),
                    },
                    PlayerProperty {
                        name: "other".to_string(),
                        value: "v".to_string(),
                        signature: None,
                    },
                ],
                game_mode: 1,
                ping: 42,
                display_name: Some(r#"{"text":"Steve","color":"red"}"#.to_string()),
            },
            PlayerListAdd {
                uuid: uuid(2),
                name: "Alex".to_string(),
                properties: vec![],
                game_mode: 0,
                ping: 0,
                display_name: None,
            },
        ]);
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn the_small_update_actions_round_trip() {
        for packet in [
            PlayerListItem::UpdateGameMode(vec![(uuid(1), 3), (uuid(2), 0)]),
            PlayerListItem::UpdateLatency(vec![(uuid(1), 120)]),
            PlayerListItem::UpdateDisplayName(vec![
                (uuid(1), Some(r#"{"text":"x"}"#.to_string())),
                (uuid(2), None),
            ]),
            PlayerListItem::Remove(vec![uuid(1), uuid(2), uuid(3)]),
        ] {
            assert_eq!(round_trip(&packet), packet);
        }
    }

    #[test]
    fn a_remove_entry_is_just_the_uuid() {
        let mut buffer = Vec::new();
        PlayerListItem::Remove(vec![uuid(7)])
            .encode_body(&mut buffer)
            .unwrap();
        assert_eq!(buffer.len(), 1 + 1 + 16);
        assert_eq!(buffer[0], 4);
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        assert!(PlayerListItem::decode(&mut &[9u8, 0][..]).is_err());
    }

    #[test]
    fn an_oversized_entry_count_is_rejected_before_allocation() {
        // Action 0 (add), then VarInt 5000.
        assert!(PlayerListItem::decode(&mut &[0u8, 0x88, 0x27][..]).is_err());
    }

    #[test]
    fn a_truncated_entry_is_an_error_not_a_panic() {
        let mut buffer = Vec::new();
        PlayerListItem::UpdateLatency(vec![(uuid(1), 5)])
            .encode_body(&mut buffer)
            .unwrap();
        buffer.truncate(buffer.len() - 3);
        assert!(PlayerListItem::decode(&mut &buffer[..]).is_err());
    }
}
