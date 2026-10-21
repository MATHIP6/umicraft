// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Teleport` (0x18, clientbound, play): an entity was placed at an
//! absolute position.
//!
//! Layout (MCP `S18PacketEntityTeleport`, `readPacketData`): VarInt entity
//! id, `i32` X, Y and Z (the position multiplied by 32), `i8` yaw and pitch
//! (degrees × 256/360), and a boolean byte for "on ground".
//!
//! 1.8 has no "relative" flag here; the position is always absolute.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_i32};
use crate::packets::play::wire::read_i8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Teleport` (0x18, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityTeleport {
    /// The entity that was teleported.
    pub entity_id: i32,
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
    /// True when the entity is standing on ground.
    pub on_ground: bool,
}

impl EntityTeleport {
    /// Wire packet id.
    pub const ID: u8 = 0x18;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            x: read_i32(reader)?,
            y: read_i32(reader)?,
            z: read_i32(reader)?,
            yaw: read_i8(reader)?,
            pitch: read_i8(reader)?,
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for EntityTeleport {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
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
    fn entity_teleport_wire_shape() {
        // Captured from a live 1.8 server.
        let bytes = [
            0xAE, 0xBD, 0x01, // entity id 24238
            0xFF, 0xFF, 0x29, 0x57, // x -54953
            0x00, 0x00, 0x00, 0x80, // y 128
            0x00, 0x00, 0x0E, 0x83, // z 3715
            0x51, // yaw 81
            0x00, // pitch 0
            0x01, // on ground
        ];
        let packet = EntityTeleport::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);
        assert_eq!(packet.x, -54_953);
        assert_eq!(packet.y, 128);
        assert_eq!(packet.z, 3715);
        assert_eq!(packet.yaw, 81);
        assert_eq!(packet.pitch, 0);
        assert!(packet.on_ground);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
