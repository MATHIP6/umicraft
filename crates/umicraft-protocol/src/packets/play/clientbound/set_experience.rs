// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Set Experience` (0x1F, clientbound, play): the player's experience
//! changed.
//!
//! Layout (MCP `S1FPacketSetExperience`, `readPacketData`): `f32`
//! experience-bar fill (0..1), VarInt experience level, VarInt total
//! experience.
//!
//! The order matters: the reference class stores the *total* experience in
//! its constructor but writes the level first on the wire.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_f32;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Set Experience` (0x1F, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetExperience {
    /// Fill of the experience bar, 0..1.
    pub experience: f32,
    /// The experience level.
    pub level: i32,
    /// The total collected experience.
    pub total: i32,
}

impl SetExperience {
    /// Wire packet id.
    pub const ID: u8 = 0x1F;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            experience: read_f32(reader)?,
            level: read_var_i32(reader)?,
            total: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for SetExperience {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.experience.to_be_bytes())?;
        write_var_u32(writer, self.level as u32)?;
        write_var_u32(writer, self.total as u32)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_experience_wire_shape() {
        // Captured from a live 1.8 server: bar 0.0, level 0, total 0.
        let bytes = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let packet = SetExperience::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.experience, 0.0);
        assert_eq!(packet.level, 0);
        assert_eq!(packet.total, 0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn the_level_precedes_the_total() {
        let bytes = [
            0x3F, 0x80, 0x00, 0x00, // bar 1.0
            0x07, // level 7
            0x2C, // total 44
        ];
        let packet = SetExperience::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.level, 7);
        assert_eq!(packet.total, 44);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
