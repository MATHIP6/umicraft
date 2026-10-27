// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Abilities` (0x39, clientbound, play): creative-mode flags and speeds.
//!
//! Layout (MCP `S39PacketPlayerAbilities`, `readPacketData`): a flags byte
//! (bit 0 invulnerable, bit 1 flying, bit 2 may fly, bit 3 creative mode),
//! then the fly speed and the walk speed as big-endian f32.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f32, read_u8};
use crate::packets::EncodablePacket;

const INVULNERABLE: u8 = 1;
const FLYING: u8 = 2;
const ALLOW_FLYING: u8 = 4;
const CREATIVE_MODE: u8 = 8;

/// `Player Abilities` (0x39, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerAbilities {
    /// Raw flags byte; use the accessor methods to read single flags.
    pub flags: u8,
    /// Flying speed (vanilla default `0.05`).
    pub fly_speed: f32,
    /// Walking speed (vanilla default `0.1`).
    pub walk_speed: f32,
}

impl PlayerAbilities {
    /// Wire packet id.
    pub const ID: u8 = 0x39;

    /// Whether the player takes no damage.
    pub fn invulnerable(&self) -> bool {
        self.flags & INVULNERABLE != 0
    }

    /// Whether the player is currently flying.
    pub fn flying(&self) -> bool {
        self.flags & FLYING != 0
    }

    /// Whether the player may toggle flight.
    pub fn allow_flying(&self) -> bool {
        self.flags & ALLOW_FLYING != 0
    }

    /// Whether the player is in creative mode (instant block breaking).
    pub fn creative_mode(&self) -> bool {
        self.flags & CREATIVE_MODE != 0
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            flags: read_u8(reader)?,
            fly_speed: read_f32(reader)?,
            walk_speed: read_f32(reader)?,
        })
    }
}

impl EncodablePacket for PlayerAbilities {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.flags])?;
        writer.write_all(&self.fly_speed.to_be_bytes())?;
        writer.write_all(&self.walk_speed.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_abilities_wire_shape_and_flags() {
        let mut expected = vec![0b1010];
        expected.extend(0.05f32.to_be_bytes());
        expected.extend(0.1f32.to_be_bytes());
        let packet = PlayerAbilities::decode(&mut &expected[..]).unwrap();
        assert!(!packet.invulnerable());
        assert!(packet.flying());
        assert!(!packet.allow_flying());
        assert!(packet.creative_mode());
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
    }

    #[test]
    fn a_truncated_player_abilities_is_an_error() {
        assert!(PlayerAbilities::decode(&mut &[0u8; 8][..]).is_err());
    }
}
