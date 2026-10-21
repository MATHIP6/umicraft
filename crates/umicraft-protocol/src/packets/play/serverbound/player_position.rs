// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Position` (0x04, serverbound, play): the player moved.
//!
//! Layout (MCP `C03PacketPlayer.C04PacketPlayerPosition`, `readPacketData`):
//! `f64` X, Y and Z — the position of the player's *feet* — followed by a
//! boolean byte for "on ground".
//!
//! The rotation is not part of this packet; `Player Look` (0x05) carries it.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_f64};
use crate::packets::EncodablePacket;

/// `Player Position` (0x04, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerPosition {
    /// World X of the player's feet.
    pub x: f64,
    /// World Y of the player's feet.
    pub y: f64,
    /// World Z of the player's feet.
    pub z: f64,
    /// True when the player claims to be standing on ground.
    pub on_ground: bool,
}

impl PlayerPosition {
    /// Wire packet id.
    pub const ID: u8 = 0x04;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            x: read_f64(reader)?,
            y: read_f64(reader)?,
            z: read_f64(reader)?,
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for PlayerPosition {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&[u8::from(self.on_ground)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_position_wire_shape() {
        let bytes = [
            0x40, 0x9A, 0xD4, 0x00, 0x00, 0x00, 0x00, 0x00, // x 1717.0
            0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // y 4.0
            0x40, 0x5A, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, // z 107.0
            0x01, // on ground
        ];
        let packet = PlayerPosition::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.x, 1717.0);
        assert_eq!(packet.y, 4.0);
        assert_eq!(packet.z, 107.0);
        assert!(packet.on_ground);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
