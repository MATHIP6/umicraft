// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Position and Look` (0x06, serverbound, play): teleport answer and
//! movement updates.
//!
//! Layout (`wiki.vg`, protocol 47): f64 x/y/z, f32 yaw/pitch, then an
//! `on_ground` boolean byte. Vanilla answers a clientbound 0x08 with this
//! packet once the new position is accepted.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_f32, read_f64};
use crate::packets::EncodablePacket;

/// `Player Position and Look` (0x06, serverbound, play).
#[derive(Debug, Clone, PartialEq)]
pub struct PositionLook {
    /// Accepted world X.
    pub x: f64,
    /// Accepted world Y.
    pub y: f64,
    /// Accepted world Z.
    pub z: f64,
    /// Accepted yaw.
    pub yaw: f32,
    /// Accepted pitch.
    pub pitch: f32,
    /// True when the client claims to be standing on ground.
    pub on_ground: bool,
}

impl PositionLook {
    /// Wire packet id.
    pub const ID: u8 = 0x06;

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
            on_ground: read_bool(reader)?,
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
        writer.write_all(&[u8::from(self.on_ground)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_look_round_trip() {
        let packet = PositionLook {
            x: 1.5,
            y: -2.5,
            z: 300.0,
            yaw: -90.0,
            pitch: 12.0,
            on_ground: false,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(PositionLook::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
