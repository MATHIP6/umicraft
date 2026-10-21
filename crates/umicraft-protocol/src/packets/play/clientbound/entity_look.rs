// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Look` (0x16, clientbound, play): an entity turned without
//! moving.
//!
//! Layout (MCP `S14PacketEntity.S16PacketEntityLook`, `readPacketData`):
//! VarInt entity id, `i8` yaw, `i8` pitch, boolean byte for "on ground".
//!
//! Both angles are the byte form of a degree value: the server writes
//! `(byte)(degrees * 256 / 360)` and the client reads it back as
//! `byte * 360 / 256`.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_bool;
use crate::packets::play::wire::read_i8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Look` (0x16, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityLook {
    /// The entity that turned.
    pub entity_id: i32,
    /// Yaw multiplied by 256/360.
    pub yaw: i8,
    /// Pitch multiplied by 256/360.
    pub pitch: i8,
    /// True when the entity is standing on ground.
    pub on_ground: bool,
}

impl EntityLook {
    /// Wire packet id.
    pub const ID: u8 = 0x16;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            yaw: read_i8(reader)?,
            pitch: read_i8(reader)?,
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for EntityLook {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
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
    fn entity_look_wire_shape() {
        // Captured from a live 1.8 server: yaw -40, pitch 0, on ground.
        let bytes = [0xA6, 0xF2, 0x01, 0xD8, 0x00, 0x01];
        let packet = EntityLook::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 31_014);
        assert_eq!(packet.yaw, -40);
        assert_eq!(packet.pitch, 0);
        assert!(packet.on_ground);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
