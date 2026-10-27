// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Global Entity` (0x2C, clientbound, play): a lightning bolt (the
//! only global entity type in 1.8) appeared.
//!
//! Layout (MCP `S2CPacketSpawnGlobalEntity`, `readPacketData`): entity id
//! (VarInt), type (i8, 1 = lightning) and x/y/z as fixed-point i32
//! (coordinate * 32).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Spawn Global Entity` (0x2C, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnGlobalEntity {
    /// Entity id.
    pub entity_id: i32,
    /// Global entity type (1 = lightning bolt).
    pub entity_type: i8,
    /// X position, fixed-point (coordinate * 32).
    pub x: i32,
    /// Y position, fixed-point (coordinate * 32).
    pub y: i32,
    /// Z position, fixed-point (coordinate * 32).
    pub z: i32,
}

impl SpawnGlobalEntity {
    /// Wire packet id.
    pub const ID: u8 = 0x2C;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            entity_type: read_i8(reader)?,
            x: read_i32(reader)?,
            y: read_i32(reader)?,
            z: read_i32(reader)?,
        })
    }
}

impl EncodablePacket for SpawnGlobalEntity {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        write_i8(writer, self.entity_type)?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_global_entity_wire_shape() {
        let mut expected = vec![0x0B, 0x01];
        expected.extend_from_slice(&64i32.to_be_bytes());
        expected.extend_from_slice(&2048i32.to_be_bytes());
        expected.extend_from_slice(&(-32i32).to_be_bytes());
        let packet = SpawnGlobalEntity {
            entity_id: 11,
            entity_type: 1,
            x: 64,
            y: 2048,
            z: -32,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            SpawnGlobalEntity::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn a_truncated_global_entity_is_an_error() {
        assert!(SpawnGlobalEntity::decode(&mut &[][..]).is_err());
        assert!(SpawnGlobalEntity::decode(&mut &[0x0B, 0x01, 0, 0][..]).is_err());
    }
}
