// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Player` (0x0C, clientbound, play): another player entered the
//! world.
//!
//! Layout (MCP `S0CPacketSpawnPlayer`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | entity id | VarInt |
//! | player UUID | 2 × i64 (most significant bits first) |
//! | x, y, z | 3 × i32, the position multiplied by 32 |
//! | yaw, pitch | 2 × i8, the angle multiplied by 256/360 |
//! | held item id | i16 |
//! | data watcher list | see [`crate::packets::play::metadata`] |
//!
//! The UUID is the plain 16-byte RFC 4122 layout: the most significant
//! bits first, then the least significant bits.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::play::metadata::Metadata;
use crate::packets::play::wire::{read_i16, read_i8};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Spawn Player` (0x0C, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnPlayer {
    /// The new player's entity id.
    pub entity_id: i32,
    /// The player's UUID, most significant bits first.
    pub uuid: [u8; 16],
    /// Position X multiplied by 32.
    pub x: i32,
    /// Position Y multiplied by 32.
    pub y: i32,
    /// Position Z multiplied by 32.
    pub z: i32,
    /// Yaw multiplied by 256/360.
    pub yaw: i8,
    /// Pitch multiplied by 256/360.
    pub pitch: i8,
    /// Numeric id of the item the player holds.
    pub held_item: i16,
    /// The player's data watcher list.
    pub metadata: Metadata,
}

impl SpawnPlayer {
    /// Wire packet id.
    pub const ID: u8 = 0x0C;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, or a malformed
    /// data watcher list.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let mut uuid = [0u8; 16];
        reader.read_exact(&mut uuid)?;
        Ok(Self {
            entity_id,
            uuid,
            x: read_i32(reader)?,
            y: read_i32(reader)?,
            z: read_i32(reader)?,
            yaw: read_i8(reader)?,
            pitch: read_i8(reader)?,
            held_item: read_i16(reader)?,
            metadata: Metadata::decode(reader)?,
        })
    }
}

impl EncodablePacket for SpawnPlayer {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.uuid)?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&self.yaw.to_be_bytes())?;
        writer.write_all(&self.pitch.to_be_bytes())?;
        writer.write_all(&self.held_item.to_be_bytes())?;
        self.metadata.encode(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_player_wire_shape() {
        let mut bytes = Vec::new();
        bytes.extend([0x01]); // entity id 1
        bytes.extend([0u8; 16]); // uuid
        bytes.extend_from_slice(&(-100i32 * 32).to_be_bytes());
        bytes.extend_from_slice(&64i32.to_be_bytes());
        bytes.extend_from_slice(&200i32.to_be_bytes());
        bytes.extend([64, 0]); // yaw, pitch
        bytes.extend_from_slice(&(-1i16).to_be_bytes()); // no held item
        bytes.push(0x7F); // empty data watcher list

        let packet = SpawnPlayer::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 1);
        assert_eq!(packet.x, -3200);
        assert_eq!(packet.y, 64);
        assert_eq!(packet.z, 200);
        assert_eq!(packet.yaw, 64);
        assert_eq!(packet.pitch, 0);
        assert_eq!(packet.held_item, -1);
        assert!(packet.metadata.objects.is_empty());

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_truncated_spawn_player_is_an_error_not_a_panic() {
        assert!(SpawnPlayer::decode(&mut &[][..]).is_err());
        assert!(SpawnPlayer::decode(&mut &[0x01, 0x00][..]).is_err());
    }
}
