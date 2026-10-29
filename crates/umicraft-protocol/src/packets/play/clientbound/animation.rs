// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Animation` (0x0B, clientbound, play): an entity swung its arm or
//! turned.
//!
//! Layout (MCP `S0BPacketAnimation`, `readPacketData`): VarInt entity id,
//! then an unsigned byte with the animation type (0 = swing the main hand,
//! 1 = damage, 2 = swing the off hand).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Animation` (0x0B, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animation {
    /// The entity that animated.
    pub entity_id: i32,
    /// Animation type (1.8): 0 = swing the arm, 1 = damage, 2 = leave the
    /// bed, 4 = critical hit, 5 = magical critical hit.
    pub animation: u8,
}

impl Animation {
    /// Wire packet id.
    pub const ID: u8 = 0x0B;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            animation: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for Animation {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&[self.animation])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_wire_shape() {
        let bytes = [0xAE, 0xBD, 0x01, 0x00];
        let packet = Animation::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);
        assert_eq!(packet.animation, 0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
