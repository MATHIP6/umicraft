// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Animation` (0x0A, serverbound, play): the player swung their arm.
//!
//! Layout (MCP `C0APacketAnimation`, `readPacketData`): **empty**. The
//! packet exists only to tell the server that the player swung; the
//! reference client sends it when the left mouse button is held.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;

/// `Animation` (0x0A, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Animation;

impl Animation {
    /// Wire packet id.
    pub const ID: u8 = 0x0A;

    /// Reads the body (which is empty).
    ///
    /// # Errors
    /// Never fails; the signature matches the other packet decoders.
    pub fn decode(_reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self)
    }
}

impl EncodablePacket for Animation {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, _writer: &mut impl Write) -> Result<(), ProtocolError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animation_body_is_empty() {
        let packet = Animation::decode(&mut &[][..]).unwrap();
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert!(buffer.is_empty());
        assert_eq!(packet.packet_id(), 0x0A);
    }
}
