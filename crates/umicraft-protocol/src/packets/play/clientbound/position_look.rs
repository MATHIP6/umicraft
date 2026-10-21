// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Position and Look` (0x08, clientbound, play): teleport and initial
//! placement.
//!
//! Layout (`wiki.vg` / MCP `S08PacketPlayerPosLook`): three f64 coordinates,
//! f32 yaw and pitch, then a raw u8 relative-flags bitfield.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f32, read_f64, read_u8};
use crate::packets::EncodablePacket;

/// `Player Position and Look` (0x08, clientbound, play).
#[derive(Debug, Clone, PartialEq)]
pub struct PositionLook {
    /// World X.
    pub x: f64,
    /// World Y.
    pub y: f64,
    /// World Z.
    pub z: f64,
    /// Yaw in degrees.
    pub yaw: f32,
    /// Pitch in degrees.
    pub pitch: f32,
    /// Relative bitfield.
    pub flags: u8,
}

impl PositionLook {
    /// Wire packet id.
    pub const ID: u8 = 0x08;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            x: read_f64(reader)?,
            y: read_f64(reader)?,
            z: read_f64(reader)?,
            yaw: read_f32(reader)?,
            pitch: read_f32(reader)?,
            flags: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for PositionLook {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&self.yaw.to_be_bytes())?;
        writer.write_all(&self.pitch.to_be_bytes())?;
        writer.write_all(&[self.flags])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_look_wire_shape() {
        let packet = PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            flags: 0,
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.extend_from_slice(&64.0f64.to_be_bytes());
        expected.extend_from_slice(&(-0.5f64).to_be_bytes());
        expected.extend_from_slice(&90.0f32.to_be_bytes());
        expected.extend_from_slice(&0.0f32.to_be_bytes());
        expected.push(0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(PositionLook::decode(&mut &expected[..]).unwrap(), packet);
    }
}
