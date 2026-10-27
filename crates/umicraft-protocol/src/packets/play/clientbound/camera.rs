// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Camera` (0x43, clientbound, play): the client now views the world from
//! another entity (spectator mode).
//!
//! Layout (MCP `S43PacketCamera`, `readPacketData`): the entity id as a
//! VarInt.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Camera` (0x43, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    /// Entity whose view the client adopts.
    pub entity_id: i32,
}

impl Camera {
    /// Wire packet id.
    pub const ID: u8 = 0x43;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for Camera {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_wire_shape() {
        let mut buffer = Vec::new();
        Camera { entity_id: 300 }.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [0xAC, 0x02]);
        assert_eq!(Camera::decode(&mut &buffer[..]).unwrap().entity_id, 300);
    }

    #[test]
    fn hostile_camera_is_an_error() {
        assert!(Camera::decode(&mut &[][..]).is_err());
        assert!(Camera::decode(&mut &[0x80][..]).is_err());
        assert!(Camera::decode(&mut &[0xFF; 6][..]).is_err());
    }
}
