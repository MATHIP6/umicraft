// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Look and Move` (0x17, clientbound, play): an entity moved and
//! turned.
//!
//! Layout (MCP `S14PacketEntity.S17PacketEntityLookMove`, `readPacketData`):
//! VarInt entity id, `i8` deltas for X, Y and Z (1/32 of a block), `i8` yaw
//! and pitch (degrees × 256/360), and a boolean byte for "on ground".

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_bool;
use crate::packets::play::wire::read_i8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Look and Move` (0x17, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityLookMove {
    /// The entity that moved.
    pub entity_id: i32,
    /// X delta in 1/32 of a block.
    pub dx: i8,
    /// Y delta in 1/32 of a block.
    pub dy: i8,
    /// Z delta in 1/32 of a block.
    pub dz: i8,
    /// Yaw multiplied by 256/360.
    pub yaw: i8,
    /// Pitch multiplied by 256/360.
    pub pitch: i8,
    /// True when the entity is standing on ground.
    pub on_ground: bool,
}

impl EntityLookMove {
    /// Wire packet id.
    pub const ID: u8 = 0x17;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            dx: read_i8(reader)?,
            dy: read_i8(reader)?,
            dz: read_i8(reader)?,
            yaw: read_i8(reader)?,
            pitch: read_i8(reader)?,
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for EntityLookMove {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.dx.to_be_bytes())?;
        writer.write_all(&self.dy.to_be_bytes())?;
        writer.write_all(&self.dz.to_be_bytes())?;
        writer.write_all(&self.yaw.to_be_bytes())?;
        writer.write_all(&self.pitch.to_be_bytes())?;
        writer.write_all(&[u8::from(self.on_ground)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_look_move_wire_shape() {
        // Captured from a live 1.8 server: dx 6, dy 5, dz -1, yaw -59, pitch 0.
        let bytes = [0xAE, 0xBD, 0x01, 0x06, 0x05, 0xFF, 0xC5, 0x00, 0x00];
        let packet = EntityLookMove::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);
        assert_eq!(packet.dx, 6);
        assert_eq!(packet.dy, 5);
        assert_eq!(packet.dz, -1);
        assert_eq!(packet.yaw, -59);
        assert_eq!(packet.pitch, 0);
        assert!(!packet.on_ground);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
