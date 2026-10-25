// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Mob` (0x0F, clientbound, play): a living entity (non-player) entered
//! the world.
//!
//! Layout (MCP `S0FPacketSpawnMob`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | entity id | VarInt |
//! | type | u8 |
//! | x, y, z | 3 × i32, the position multiplied by 32 |
//! | yaw, pitch, head pitch | 3 × i8, the angles multiplied by 256/360 |
//! | velocity x, y, z | 3 × i16 |
//! | data watcher list | see [`crate::packets::play::metadata`] |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::play::metadata::Metadata;
use crate::packets::play::wire::{read_i16, read_i8};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Spawn Mob` (0x0F, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnMob {
    /// Entity ID.
    pub entity_id: i32,
    /// Mob type numeric identifier (e.g. 50 Creeper, 51 Skeleton, 54 Zombie, 92 Cow).
    pub mob_type: u8,
    /// Position X multiplied by 32.
    pub x: i32,
    /// Position Y multiplied by 32.
    pub y: i32,
    /// Position Z multiplied by 32.
    pub z: i32,
    /// Yaw angle multiplied by 256/360.
    pub yaw: i8,
    /// Pitch angle multiplied by 256/360.
    pub pitch: i8,
    /// Head yaw angle multiplied by 256/360.
    pub head_yaw: i8,
    /// Velocity X.
    pub velocity_x: i16,
    /// Velocity Y.
    pub velocity_y: i16,
    /// Velocity Z.
    pub velocity_z: i16,
    /// The mob's data watcher list.
    pub metadata: Metadata,
}

impl SpawnMob {
    /// Wire packet id.
    pub const ID: u8 = 0x0F;

    /// Reads the packet body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, malformed VarInt, or invalid metadata.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let mut mob_type_buf = [0u8; 1];
        reader.read_exact(&mut mob_type_buf)?;
        let mob_type = mob_type_buf[0];

        let x = read_i32(reader)?;
        let y = read_i32(reader)?;
        let z = read_i32(reader)?;
        let yaw = read_i8(reader)?;
        let pitch = read_i8(reader)?;
        let head_yaw = read_i8(reader)?;
        let velocity_x = read_i16(reader)?;
        let velocity_y = read_i16(reader)?;
        let velocity_z = read_i16(reader)?;
        let metadata = Metadata::decode(reader)?;

        Ok(Self {
            entity_id,
            mob_type,
            x,
            y,
            z,
            yaw,
            pitch,
            head_yaw,
            velocity_x,
            velocity_y,
            velocity_z,
            metadata,
        })
    }
}

impl EncodablePacket for SpawnMob {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&[self.mob_type])?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&self.yaw.to_be_bytes())?;
        writer.write_all(&self.pitch.to_be_bytes())?;
        writer.write_all(&self.head_yaw.to_be_bytes())?;
        writer.write_all(&self.velocity_x.to_be_bytes())?;
        writer.write_all(&self.velocity_y.to_be_bytes())?;
        writer.write_all(&self.velocity_z.to_be_bytes())?;
        self.metadata.encode(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_mob_roundtrip() {
        let packet = SpawnMob {
            entity_id: 1042,
            mob_type: 54, // Zombie
            x: 3200,
            y: 2048,
            z: -1600,
            yaw: 64,
            pitch: 0,
            head_yaw: 64,
            velocity_x: 100,
            velocity_y: -200,
            velocity_z: 300,
            metadata: Metadata::default(),
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = SpawnMob::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }
}
