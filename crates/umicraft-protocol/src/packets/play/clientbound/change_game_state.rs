// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Change Game State` (0x2B, clientbound, play): the server changed a
//! global game-state value.
//!
//! Layout (MCP `S2BPacketChangeGameState`, `readPacketData`): an unsigned
//! byte with the state id, followed by an `f32` value.
//!
//! The states the reference client reacts to are 0 (the sun/moon is
//! ignored), 1 (the rain toggle), 2 (the thunder toggle) and 3 (the rain
//! level, 0..1).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f32, read_u8};
use crate::packets::EncodablePacket;

/// `Change Game State` (0x2B, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChangeGameState {
    /// The state that changed: 1 = rain toggle, 2 = thunder toggle,
    /// 3 = rain level.
    pub reason: u8,
    /// The new value.
    pub value: f32,
}

impl ChangeGameState {
    /// Wire packet id.
    pub const ID: u8 = 0x2B;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            reason: read_u8(reader)?,
            value: read_f32(reader)?,
        })
    }
}

impl EncodablePacket for ChangeGameState {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.reason])?;
        writer.write_all(&self.value.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_game_state_wire_shape() {
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x00];
        let packet = ChangeGameState::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.reason, 1);
        assert_eq!(packet.value, 0.0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn the_rain_level_carries_a_fraction() {
        let bytes = [0x03, 0x3F, 0x80, 0x00, 0x00];
        let packet = ChangeGameState::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.reason, 3);
        assert_eq!(packet.value, 1.0);
    }
}
