// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Object` (0x0E, clientbound, play): a non-living entity (item, arrow,
//! vehicle, etc.) entered the world.
//!
//! Layout (MCP `S0EPacketSpawnObject`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | entity id | VarInt |
//! | type | u8 |
//! | x, y, z | 3 × i32, the position multiplied by 32 |
//! | pitch, yaw | 2 × i8, the angles multiplied by 256/360 |
//! | data | i32 |
//! | speed x, y, z | 3 × i16 (present only when data > 0) |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::play::wire::{read_i16, read_i8};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Spawn Object` (0x0E, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnObject {
    /// Entity ID.
    pub entity_id: i32,
    /// Object type numeric identifier (e.g. 2 for Item, 60 for Arrow, 70 for Falling Block).
    pub object_type: u8,
    /// Position X multiplied by 32.
    pub x: i32,
    /// Position Y multiplied by 32.
    pub y: i32,
    /// Position Z multiplied by 32.
    pub z: i32,
    /// Pitch angle multiplied by 256/360.
    pub pitch: i8,
    /// Yaw angle multiplied by 256/360.
    pub yaw: i8,
    /// Object-specific data integer.
    pub data: i32,
    /// Speed X (0 if data <= 0).
    pub velocity_x: i16,
    /// Speed Y (0 if data <= 0).
    pub velocity_y: i16,
    /// Speed Z (0 if data <= 0).
    pub velocity_z: i16,
}

impl SpawnObject {
    /// Wire packet id.
    pub const ID: u8 = 0x0E;

    /// Reads the packet body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let mut object_type_buf = [0u8; 1];
        reader.read_exact(&mut object_type_buf)?;
        let object_type = object_type_buf[0];

        let x = read_i32(reader)?;
        let y = read_i32(reader)?;
        let z = read_i32(reader)?;
        let pitch = read_i8(reader)?;
        let yaw = read_i8(reader)?;
        let data = read_i32(reader)?;

        let (velocity_x, velocity_y, velocity_z) = if data > 0 {
            (read_i16(reader)?, read_i16(reader)?, read_i16(reader)?)
        } else {
            (0, 0, 0)
        };

        Ok(Self {
            entity_id,
            object_type,
            x,
            y,
            z,
            pitch,
            yaw,
            data,
            velocity_x,
            velocity_y,
            velocity_z,
        })
    }
}

impl EncodablePacket for SpawnObject {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&[self.object_type])?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&self.pitch.to_be_bytes())?;
        writer.write_all(&self.yaw.to_be_bytes())?;
        writer.write_all(&self.data.to_be_bytes())?;
        if self.data > 0 {
            writer.write_all(&self.velocity_x.to_be_bytes())?;
            writer.write_all(&self.velocity_y.to_be_bytes())?;
            writer.write_all(&self.velocity_z.to_be_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_object_without_velocity_roundtrip() {
        let packet = SpawnObject {
            entity_id: 50,
            object_type: 2, // Item
            x: 100,
            y: 200,
            z: 300,
            pitch: 0,
            yaw: 0,
            data: 0,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = SpawnObject::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }

    #[test]
    fn spawn_object_with_velocity_roundtrip() {
        let packet = SpawnObject {
            entity_id: 51,
            object_type: 60, // Arrow
            x: 100,
            y: 200,
            z: 300,
            pitch: 10,
            yaw: 20,
            data: 1,
            velocity_x: 500,
            velocity_y: 200,
            velocity_z: -300,
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = SpawnObject::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }
}
