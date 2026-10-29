// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Remove Entity Effect` (0x1E, clientbound, play): a potion effect ended.
//!
//! Layout (MCP `S1EPacketRemoveEntityEffect`, `readPacketData`): entity id
//! (VarInt) and the effect id as an unsigned byte.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Remove Entity Effect` (0x1E, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveEntityEffect {
    /// Entity the effect is removed from.
    pub entity_id: i32,
    /// Potion effect id.
    pub effect_id: u8,
}

impl RemoveEntityEffect {
    /// Wire packet id.
    pub const ID: u8 = 0x1E;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            effect_id: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for RemoveEntityEffect {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&[self.effect_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_entity_effect_wire_shape() {
        let expected = [0x04, 0x10];
        let packet = RemoveEntityEffect {
            entity_id: 4,
            effect_id: 16,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            RemoveEntityEffect::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn a_truncated_removal_is_an_error() {
        assert!(RemoveEntityEffect::decode(&mut &[][..]).is_err());
        assert!(RemoveEntityEffect::decode(&mut &[0x04][..]).is_err());
    }
}
