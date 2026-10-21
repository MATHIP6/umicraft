// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Look` (0x05, serverbound, play): the player turned.
//!
//! Layout (MCP `C03PacketPlayer.C05PacketPlayerLook`, `readPacketData`):
//! `f32` yaw and pitch in degrees, followed by a boolean byte for "on
//! ground".
//!
//! The position is not part of this packet; `Player Position` (0x04) carries
//! it.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_f32};
use crate::packets::EncodablePacket;

/// `Player Look` (0x05, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerLook {
    /// Yaw in degrees.
    pub yaw: f32,
    /// Pitch in degrees.
    pub pitch: f32,
    /// True when the player claims to be standing on ground.
    pub on_ground: bool,
}

impl PlayerLook {
    /// Wire packet id.
    pub const ID: u8 = 0x05;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            yaw: read_f32(reader)?,
            pitch: read_f32(reader)?,
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for PlayerLook {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
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
    fn player_look_wire_shape() {
        let bytes = [
            0x42, 0xB4, 0x00, 0x00, // yaw 90.0
            0x00, 0x00, 0x00, 0x00, // pitch 0.0
            0x00, // not on ground
        ];
        let packet = PlayerLook::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.yaw, 90.0);
        assert_eq!(packet.pitch, 0.0);
        assert!(!packet.on_ground);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
