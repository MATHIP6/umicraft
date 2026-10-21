// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Input` (0x0C, serverbound, play): the player's movement keys.
//!
//! Layout (MCP `C0CPacketInput`, `readPacketData`): `f32` strafe speed,
//! `f32` forward speed, and one byte of flags — bit 0 is "jumping", bit 1 is
//! "sneaking".
//!
//! The reference client sends positive strafe for a left strafe and negative
//! for a right one; positive forward speed means "forward". The values are
//! the raw key presses, scaled by the client (0.2 for walking, 1.0 for
//! sprinting).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_f32;
use crate::packets::EncodablePacket;

/// `Input` (0x0C, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Input {
    /// Strafe speed: positive to the left, negative to the right.
    pub strafe: f32,
    /// Forward speed: positive forwards, negative backwards.
    pub forward: f32,
    /// True while the jump key is held.
    pub jumping: bool,
    /// True while the sneak key is held.
    pub sneaking: bool,
}

impl Input {
    /// Wire packet id.
    pub const ID: u8 = 0x0C;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let strafe = read_f32(reader)?;
        let forward = read_f32(reader)?;
        let mut flags = [0u8; 1];
        reader.read_exact(&mut flags)?;
        Ok(Self {
            strafe,
            forward,
            jumping: flags[0] & 1 != 0,
            sneaking: flags[0] & 2 != 0,
        })
    }
}

impl EncodablePacket for Input {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.strafe.to_be_bytes())?;
        writer.write_all(&self.forward.to_be_bytes())?;
        let mut flags = 0u8;
        if self.jumping {
            flags |= 1;
        }
        if self.sneaking {
            flags |= 2;
        }
        writer.write_all(&[flags])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_wire_shape() {
        let bytes = [
            0x00, 0x00, 0x00, 0x00, // strafe 0.0
            0x00, 0x00, 0x00, 0x00, // forward 0.0
            0x03, // jumping and sneaking
        ];
        let packet = Input::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.strafe, 0.0);
        assert_eq!(packet.forward, 0.0);
        assert!(packet.jumping);
        assert!(packet.sneaking);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn the_flag_bits_are_independent() {
        let bytes = [
            0x3D, 0xCC, 0xCC, 0xCD, // strafe 0.1
            0x3D, 0xCC, 0xCC, 0xCD, // forward 0.1
            0x01, // jumping only
        ];
        let packet = Input::decode(&mut &bytes[..]).unwrap();
        assert!(packet.jumping);
        assert!(!packet.sneaking);
        assert!((packet.strafe - 0.1f32).abs() < f32::EPSILON);
        assert!((packet.forward - 0.1f32).abs() < f32::EPSILON);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
