// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Velocity` (0x12, clientbound, play): sets velocity on an entity.
//!
//! Layout (MCP `S12PacketEntityVelocity`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | entity id | VarInt |
//! | velocity x, y, z | 3 × i16 (motion * 8000) |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::read_i16;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Velocity` (0x12, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityVelocity {
    /// Entity ID.
    pub entity_id: i32,
    /// Velocity X (fixed point, value / 8000.0 is blocks per tick).
    pub velocity_x: i16,
    /// Velocity Y (fixed point, value / 8000.0 is blocks per tick).
    pub velocity_y: i16,
    /// Velocity Z (fixed point, value / 8000.0 is blocks per tick).
    pub velocity_z: i16,
}

impl EntityVelocity {
    /// Wire packet id.
    pub const ID: u8 = 0x12;

    /// Reads the packet body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let velocity_x = read_i16(reader)?;
        let velocity_y = read_i16(reader)?;
        let velocity_z = read_i16(reader)?;
        Ok(Self {
            entity_id,
            velocity_x,
            velocity_y,
            velocity_z,
        })
    }
}

impl EncodablePacket for EntityVelocity {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.velocity_x.to_be_bytes())?;
        writer.write_all(&self.velocity_y.to_be_bytes())?;
        writer.write_all(&self.velocity_z.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_roundtrip() {
        let packet = EntityVelocity {
            entity_id: 123,
            velocity_x: 4000,
            velocity_y: -8000,
            velocity_z: 1200,
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = EntityVelocity::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }
}
