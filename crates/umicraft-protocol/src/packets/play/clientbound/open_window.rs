// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Open Window` (0x2D, clientbound, play): the server opens a container GUI.
//!
//! Layout (MCP `S2DPacketOpenWindow`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | u8 | |
//! | inventory type | String(32) | e.g. `"minecraft:chest"`, `"EntityHorse"` |
//! | window title | String(256) | JSON chat component |
//! | slot count | u8 | |
//! | entity id | i32 | present only when `inventory_type == "EntityHorse"` |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_i32, read_u8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, read_string_capped, write_string, write_string_capped};

/// `Open Window` (0x2D, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWindow {
    /// The unique id for this window; the player inventory is always 0.
    pub window_id: u8,
    /// The type string, e.g. `"minecraft:chest"` or `"EntityHorse"`.
    pub window_type: String,
    /// The window title as a JSON chat component.
    pub title: String,
    /// The number of container slots (not counting the player inventory).
    pub slot_count: u8,
    /// The horse entity id; present only for `"EntityHorse"` windows.
    pub entity_id: Option<i32>,
}

impl OpenWindow {
    /// Wire packet id.
    pub const ID: u8 = 0x2D;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation,
    /// [`ProtocolError::Invariant`] on invalid fields.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let window_id = read_u8(reader)?;
        let window_type = read_string(reader, 32)?;
        let title = read_string_capped(reader)?;
        let slot_count = read_u8(reader)?;
        let entity_id = if window_type == "EntityHorse" {
            Some(read_i32(reader)?)
        } else {
            None
        };
        Ok(Self {
            window_id,
            window_type,
            title,
            slot_count,
            entity_id,
        })
    }
}

impl EncodablePacket for OpenWindow {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.window_id])?;
        write_string(writer, &self.window_type, 32)?;
        write_string_capped(writer, &self.title)?;
        writer.write_all(&[self.slot_count])?;
        if self.window_type == "EntityHorse" {
            if let Some(eid) = self.entity_id {
                writer.write_all(&eid.to_be_bytes())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &OpenWindow) -> OpenWindow {
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        OpenWindow::decode(&mut &buf[..]).unwrap()
    }

    #[test]
    fn a_chest_round_trips() {
        let packet = OpenWindow {
            window_id: 1,
            window_type: "minecraft:chest".to_string(),
            title: r#"{"text":"Chest"}"#.to_string(),
            slot_count: 27,
            entity_id: None,
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn a_horse_carries_its_entity_id() {
        let packet = OpenWindow {
            window_id: 2,
            window_type: "EntityHorse".to_string(),
            title: r#"{"text":"Horse"}"#.to_string(),
            slot_count: 2,
            entity_id: Some(42),
        };
        assert_eq!(round_trip(&packet), packet);
    }
}
