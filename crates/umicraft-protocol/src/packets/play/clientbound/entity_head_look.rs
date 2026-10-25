// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Head Look` (0x19, clientbound, play): sets the head yaw of an entity.
//!
//! Layout (MCP `S19PacketEntityHeadLook`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | entity id | VarInt |
//! | head yaw | i8 (multiplied by 256/360) |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::read_i8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Head Look` (0x19, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityHeadLook {
    /// Entity ID.
    pub entity_id: i32,
    /// Head yaw angle multiplied by 256/360.
    pub head_yaw: i8,
}

impl EntityHeadLook {
    /// Wire packet id.
    pub const ID: u8 = 0x19;

    /// Reads the packet body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let head_yaw = read_i8(reader)?;
        Ok(Self {
            entity_id,
            head_yaw,
        })
    }
}

impl EncodablePacket for EntityHeadLook {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.head_yaw.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_look_roundtrip() {
        let packet = EntityHeadLook {
            entity_id: 88,
            head_yaw: -32,
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = EntityHeadLook::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }
}
