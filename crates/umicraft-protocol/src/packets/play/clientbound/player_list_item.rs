// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player List Item` (0x38, clientbound, play): the tab list.
//!
//! Layout (MCP `S38PacketPlayerListItem`, `readPacketData`): a VarInt action
//! (0 add player, 1 update game mode, 2 update latency, 3 update display
//! name, 4 remove player), a VarInt entry count, then per entry a UUID (two
//! big-endian i64, most significant first) and the action's fields:
//!
//! * add player: name (16 chars), a VarInt property count with per property
//!   name (32767), value (32767), a bool and, when set, a signature (32767);
//!   then game mode VarInt, ping VarInt, a bool and, when set, the display
//!   name as chat JSON (32767);
//! * update game mode: game mode VarInt;
//! * update latency: ping VarInt;
//! * update display name: a bool and, when set, chat JSON;
//! * remove player: nothing.
//!
//! The reference client indexes an enum with the action and crashes on an
//! unknown one; here it is [`ProtocolError::Invariant`]. Game modes stay
//! raw VarInts (the reference client maps unknown ones to survival).

use std::io::{Read, Write};

use super::statistics::read_list_len;
use crate::error::ProtocolError;
use crate::packets::codec::read_bool;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

const MAX_NAME_CHARS: usize = 16;
const MAX_TEXT_CHARS: usize = 32767;

/// A skin or other profile property of a player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerProperty {
    /// Property name, e.g. `textures`.
    pub name: String,
    /// Property value (base64 for `textures`).
    pub value: String,
    /// Mojang signature of the value, when the property is signed.
    pub signature: Option<String>,
}

/// Entry of an add-player action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlayerEntry {
    /// Player UUID.
    pub uuid: u128,
    /// Player name.
    pub name: String,
    /// Profile properties.
    pub properties: Vec<PlayerProperty>,
    /// Game mode id (0 survival, 1 creative, 2 adventure, 3 spectator).
    pub game_mode: i32,
    /// Latency in milliseconds.
    pub ping: i32,
    /// Tab-list name as chat JSON, when it differs from the player name.
    pub display_name: Option<String>,
}

/// `Player List Item` (0x38, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerListItem {
    /// Action 0: players join the list.
    AddPlayer(Vec<AddPlayerEntry>),
    /// Action 1: `(uuid, game mode)` pairs.
    UpdateGameMode(Vec<(u128, i32)>),
    /// Action 2: `(uuid, ping)` pairs.
    UpdateLatency(Vec<(u128, i32)>),
    /// Action 3: `(uuid, display name chat JSON)`; `None` resets the name.
    UpdateDisplayName(Vec<(u128, Option<String>)>),
    /// Action 4: players leave the list.
    RemovePlayer(Vec<u128>),
}

fn read_uuid(reader: &mut impl Read) -> Result<u128, ProtocolError> {
    let mut bytes = [0u8; 16];
    reader.read_exact(&mut bytes)?;
    Ok(u128::from_be_bytes(bytes))
}

fn write_uuid(writer: &mut impl Write, uuid: u128) -> Result<(), ProtocolError> {
    writer.write_all(&uuid.to_be_bytes())?;
    Ok(())
}

fn read_optional_text(reader: &mut impl Read) -> Result<Option<String>, ProtocolError> {
    if read_bool(reader)? {
        Ok(Some(read_string(reader, MAX_TEXT_CHARS)?))
    } else {
        Ok(None)
    }
}

fn write_optional_text(writer: &mut impl Write, text: Option<&str>) -> Result<(), ProtocolError> {
    writer.write_all(&[u8::from(text.is_some())])?;
    if let Some(text) = text {
        write_string(writer, text, MAX_TEXT_CHARS)?;
    }
    Ok(())
}

/// Reads `count` entries with `read_entry`; the count is validated first.
fn read_entries<T, R: Read>(
    reader: &mut R,
    read_entry: impl Fn(&mut R) -> Result<T, ProtocolError>,
) -> Result<Vec<T>, ProtocolError> {
    let count = read_list_len(reader)?;
    let mut entries = Vec::new();
    for _ in 0..count {
        entries.push(read_entry(reader)?);
    }
    Ok(entries)
}

impl PlayerProperty {
    fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let name = read_string(reader, MAX_TEXT_CHARS)?;
        let value = read_string(reader, MAX_TEXT_CHARS)?;
        Ok(Self {
            name,
            value,
            signature: read_optional_text(reader)?,
        })
    }

    fn encode(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.name, MAX_TEXT_CHARS)?;
        write_string(writer, &self.value, MAX_TEXT_CHARS)?;
        write_optional_text(writer, self.signature.as_deref())
    }
}

impl AddPlayerEntry {
    fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let uuid = read_uuid(reader)?;
        let name = read_string(reader, MAX_NAME_CHARS)?;
        let properties = read_entries(reader, |r| PlayerProperty::decode(r))?;
        Ok(Self {
            uuid,
            name,
            properties,
            game_mode: read_var_i32(reader)?,
            ping: read_var_i32(reader)?,
            display_name: read_optional_text(reader)?,
        })
    }

    fn encode(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_uuid(writer, self.uuid)?;
        write_string(writer, &self.name, MAX_NAME_CHARS)?;
        write_var_u32(writer, self.properties.len() as u32)?;
        for property in &self.properties {
            property.encode(writer)?;
        }
        write_var_u32(writer, self.game_mode as u32)?;
        write_var_u32(writer, self.ping as u32)?;
        write_optional_text(writer, self.display_name.as_deref())
    }
}

impl PlayerListItem {
    /// Wire packet id.
    pub const ID: u8 = 0x38;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a bad count, an invalid string or an
    /// unknown action.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        match read_var_i32(reader)? {
            0 => Ok(Self::AddPlayer(read_entries(reader, |r| {
                AddPlayerEntry::decode(r)
            })?)),
            1 => Ok(Self::UpdateGameMode(read_entries(reader, |r| {
                Ok((read_uuid(r)?, read_var_i32(r)?))
            })?)),
            2 => Ok(Self::UpdateLatency(read_entries(reader, |r| {
                Ok((read_uuid(r)?, read_var_i32(r)?))
            })?)),
            3 => Ok(Self::UpdateDisplayName(read_entries(reader, |r| {
                Ok((read_uuid(r)?, read_optional_text(r)?))
            })?)),
            4 => Ok(Self::RemovePlayer(read_entries(reader, |r| read_uuid(r))?)),
            other => Err(ProtocolError::Invariant(format!(
                "unknown player list action {other}"
            ))),
        }
    }
}

fn write_header(writer: &mut impl Write, action: u32, count: usize) -> Result<(), ProtocolError> {
    write_var_u32(writer, action)?;
    write_var_u32(writer, count as u32)
}

impl EncodablePacket for PlayerListItem {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match self {
            Self::AddPlayer(entries) => {
                write_header(writer, 0, entries.len())?;
                entries.iter().try_for_each(|entry| entry.encode(writer))
            }
            Self::UpdateGameMode(entries) => {
                write_header(writer, 1, entries.len())?;
                entries.iter().try_for_each(|&(uuid, mode)| {
                    write_uuid(writer, uuid)?;
                    write_var_u32(writer, mode as u32)
                })
            }
            Self::UpdateLatency(entries) => {
                write_header(writer, 2, entries.len())?;
                entries.iter().try_for_each(|&(uuid, ping)| {
                    write_uuid(writer, uuid)?;
                    write_var_u32(writer, ping as u32)
                })
            }
            Self::UpdateDisplayName(entries) => {
                write_header(writer, 3, entries.len())?;
                entries.iter().try_for_each(|(uuid, name)| {
                    write_uuid(writer, *uuid)?;
                    write_optional_text(writer, name.as_deref())
                })
            }
            Self::RemovePlayer(uuids) => {
                write_header(writer, 4, uuids.len())?;
                uuids.iter().try_for_each(|&uuid| write_uuid(writer, uuid))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: u128 = 0x0011_2233_4455_6677_8899_AABB_CCDD_EEFF;

    fn uuid_bytes() -> Vec<u8> {
        UUID.to_be_bytes().to_vec()
    }

    fn check(packet: PlayerListItem, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(PlayerListItem::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn add_player_wire_shape() {
        let mut expected = vec![0, 1];
        expected.extend(uuid_bytes());
        expected.extend([3, b'B', b'o', b'b']);
        expected.extend([2]); // two properties
        expected.extend([1, b'n', 1, b'v', 1, 1, b's']); // signed
        expected.extend([1, b'm', 1, b'w', 0]); // unsigned
        expected.extend([1, 40]); // game mode, ping
        expected.extend([1, 2, b'{', b'}']); // display name
        check(
            PlayerListItem::AddPlayer(vec![AddPlayerEntry {
                uuid: UUID,
                name: "Bob".to_string(),
                properties: vec![
                    PlayerProperty {
                        name: "n".to_string(),
                        value: "v".to_string(),
                        signature: Some("s".to_string()),
                    },
                    PlayerProperty {
                        name: "m".to_string(),
                        value: "w".to_string(),
                        signature: None,
                    },
                ],
                game_mode: 1,
                ping: 40,
                display_name: Some("{}".to_string()),
            }]),
            &expected,
        );
    }

    #[test]
    fn update_actions_wire_shapes() {
        let mut expected = vec![1, 1];
        expected.extend(uuid_bytes());
        expected.push(3);
        check(PlayerListItem::UpdateGameMode(vec![(UUID, 3)]), &expected);

        let mut expected = vec![2, 1];
        expected.extend(uuid_bytes());
        expected.extend([0xAC, 0x02]);
        check(PlayerListItem::UpdateLatency(vec![(UUID, 300)]), &expected);

        let mut expected = vec![3, 2];
        expected.extend(uuid_bytes());
        expected.push(0);
        expected.extend(uuid_bytes());
        expected.extend([1, 1, b'x']);
        check(
            PlayerListItem::UpdateDisplayName(vec![(UUID, None), (UUID, Some("x".to_string()))]),
            &expected,
        );

        let mut expected = vec![4, 1];
        expected.extend(uuid_bytes());
        check(PlayerListItem::RemovePlayer(vec![UUID]), &expected);
    }

    #[test]
    fn hostile_player_list_item_is_an_error() {
        assert!(PlayerListItem::decode(&mut &[][..]).is_err());
        assert!(PlayerListItem::decode(&mut &[5, 0][..]).is_err());
        assert!(PlayerListItem::decode(&mut &[4, 1, 0, 0][..]).is_err());
        assert!(PlayerListItem::decode(&mut &[4, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F][..]).is_err());
        assert!(PlayerListItem::decode(&mut &[4, 0xFF, 0xFF, 0xFF, 0xFF, 0x07][..]).is_err());
        // Add player with an absurd property count.
        let mut bytes = vec![0, 1];
        bytes.extend(uuid_bytes());
        bytes.extend([0, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
        assert!(PlayerListItem::decode(&mut &bytes[..]).is_err());
    }
}
