// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player` (0x03, serverbound, play): the on-ground flag alone, sent by the
//! vanilla client on ticks without movement or rotation.
//!
//! Layout (MCP `C03PacketPlayer`, `writePacketData`): one byte, 1 when the
//! player is on the ground. The movement variants (0x04-0x06) append this
//! same byte after their own fields.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_bool;
use crate::packets::EncodablePacket;

/// `Player` (0x03, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Player {
    /// Whether the player stands on the ground.
    pub on_ground: bool,
}

impl Player {
    /// Wire packet id.
    pub const ID: u8 = 0x03;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for Player {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[u8::from(self.on_ground)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_wire_shape() {
        let packet = Player::decode(&mut &[0x01][..]).unwrap();
        assert!(packet.on_ground);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [0x01]);
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        assert!(Player::decode(&mut &[][..]).is_err());
    }
}
