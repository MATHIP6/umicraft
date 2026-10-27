// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Abilities` (0x13, serverbound, play): the client reports its
//! flying state.
//!
//! Layout (MCP `C13PacketPlayerAbilities`, `writePacketData`): a flags byte,
//! then the fly speed and the walk speed as big-endian f32.
//!
//! | bit | flag |
//! |---|---|
//! | 0x01 | invulnerable |
//! | 0x02 | flying |
//! | 0x04 | allow flying |
//! | 0x08 | creative mode |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f32, read_u8};
use crate::packets::EncodablePacket;

const INVULNERABLE: u8 = 0x01;
const FLYING: u8 = 0x02;
const ALLOW_FLYING: u8 = 0x04;
const CREATIVE_MODE: u8 = 0x08;
/// Every bit the reference client defines; the others are dropped on decode
/// because the reference never writes them.
const KNOWN_FLAGS: u8 = INVULNERABLE | FLYING | ALLOW_FLYING | CREATIVE_MODE;

/// `Player Abilities` (0x13, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerAbilities {
    /// The raw flags byte; use the accessors and builders.
    pub flags: u8,
    /// Flying speed.
    pub fly_speed: f32,
    /// Walking speed.
    pub walk_speed: f32,
}

impl PlayerAbilities {
    /// Wire packet id.
    pub const ID: u8 = 0x13;

    /// Abilities with no flag set and the given speeds.
    #[must_use]
    pub const fn new(fly_speed: f32, walk_speed: f32) -> Self {
        Self {
            flags: 0,
            fly_speed,
            walk_speed,
        }
    }

    /// Whether the invulnerable flag is set.
    #[must_use]
    pub const fn invulnerable(&self) -> bool {
        self.flags & INVULNERABLE != 0
    }

    /// Whether the player is flying.
    #[must_use]
    pub const fn flying(&self) -> bool {
        self.flags & FLYING != 0
    }

    /// Whether the player may fly.
    #[must_use]
    pub const fn allow_flying(&self) -> bool {
        self.flags & ALLOW_FLYING != 0
    }

    /// Whether the player is in creative mode.
    #[must_use]
    pub const fn creative_mode(&self) -> bool {
        self.flags & CREATIVE_MODE != 0
    }

    /// Returns a copy with the invulnerable flag set to `on`.
    #[must_use]
    pub const fn with_invulnerable(self, on: bool) -> Self {
        self.with_flag(INVULNERABLE, on)
    }

    /// Returns a copy with the flying flag set to `on`.
    #[must_use]
    pub const fn with_flying(self, on: bool) -> Self {
        self.with_flag(FLYING, on)
    }

    /// Returns a copy with the allow-flying flag set to `on`.
    #[must_use]
    pub const fn with_allow_flying(self, on: bool) -> Self {
        self.with_flag(ALLOW_FLYING, on)
    }

    /// Returns a copy with the creative-mode flag set to `on`.
    #[must_use]
    pub const fn with_creative_mode(self, on: bool) -> Self {
        self.with_flag(CREATIVE_MODE, on)
    }

    const fn with_flag(mut self, bit: u8, on: bool) -> Self {
        if on {
            self.flags |= bit;
        } else {
            self.flags &= !bit;
        }
        self
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            flags: read_u8(reader)? & KNOWN_FLAGS,
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
        writer.write_all(&[self.flags & KNOWN_FLAGS])?;
        writer.write_all(&self.fly_speed.to_be_bytes())?;
        writer.write_all(&self.walk_speed.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_abilities_wire_shape() {
        let packet = PlayerAbilities::new(0.05, 0.1)
            .with_flying(true)
            .with_allow_flying(true);
        assert_eq!(packet.flags, 0x06);
        let mut expected = vec![0x06];
        expected.extend_from_slice(&0.05f32.to_be_bytes());
        expected.extend_from_slice(&0.1f32.to_be_bytes());

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(PlayerAbilities::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn the_flag_bits_follow_the_reference_order() {
        let all = PlayerAbilities::new(0.0, 0.0)
            .with_invulnerable(true)
            .with_flying(true)
            .with_allow_flying(true)
            .with_creative_mode(true);
        assert_eq!(all.flags, 0x0F);
        let cleared = all.with_flying(false);
        assert!(!cleared.flying());
        assert!(cleared.invulnerable() && cleared.allow_flying() && cleared.creative_mode());
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        assert!(PlayerAbilities::decode(&mut &[0x02, 0, 0, 0, 0][..]).is_err());
    }
}
