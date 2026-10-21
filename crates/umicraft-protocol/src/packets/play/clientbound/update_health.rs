// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Health` (0x06, clientbound, play): health, food and saturation.
//!
//! Layout (MCP `S06PacketUpdateHealth`, `readPacketData`): `f32` health,
//! **VarInt** food level, `f32` saturation.
//!
//! The food level is a VarInt, not a float: the reference client reads it
//! with `readVarIntFromBuffer` and the vanilla server sends `20` as one
//! byte. A live capture confirms the 9-byte body `20.0f | 20 | 5.0f`.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_f32;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Update Health` (0x06, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpdateHealth {
    /// Health in half-hearts (20.0 = full).
    pub health: f32,
    /// Food level (20 = full).
    pub food: i32,
    /// Saturation level.
    pub saturation: f32,
}

impl UpdateHealth {
    /// Wire packet id.
    pub const ID: u8 = 0x06;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            health: read_f32(reader)?,
            food: read_var_i32(reader)?,
            saturation: read_f32(reader)?,
        })
    }
}

impl EncodablePacket for UpdateHealth {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.health.to_be_bytes())?;
        write_var_u32(writer, self.food as u32)?;
        writer.write_all(&self.saturation.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_health_wire_shape() {
        // Captured from a live 1.8 server: health 20.0, food 20, saturation 5.0.
        let bytes = [
            0x41, 0xA0, 0x00, 0x00, // health 20.0
            0x14, // food 20 (VarInt)
            0x40, 0xA0, 0x00, 0x00, // saturation 5.0
        ];
        let packet = UpdateHealth::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.health, 20.0);
        assert_eq!(packet.food, 20);
        assert_eq!(packet.saturation, 5.0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_multi_byte_food_level_uses_the_varint_encoding() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0.0f32.to_be_bytes());
        crate::varint::write_var_u32(&mut bytes, 300).unwrap();
        bytes.extend_from_slice(&0.0f32.to_be_bytes());

        let packet = UpdateHealth::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.food, 300);
    }
}
