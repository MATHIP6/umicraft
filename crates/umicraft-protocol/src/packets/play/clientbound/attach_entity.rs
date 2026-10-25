// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Attach Entity` (0x1B, clientbound, play): attaches an entity to another
//! (mounting or leashing).
//!
//! Layout (MCP `S1BPacketAttachEntity`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | leash | u8 (0 = riding, 1 = leash) |
//! | entity id | i32 |
//! | vehicle entity id | i32 |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::EncodablePacket;

/// `Attach Entity` (0x1B, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachEntity {
    /// Leash flag (0 = riding / mount, 1 = leash).
    pub leash: u8,
    /// The attached entity ID.
    pub entity_id: i32,
    /// The vehicle or leash holder entity ID (-1 if detached).
    pub vehicle_id: i32,
}

impl AttachEntity {
    /// Wire packet id.
    pub const ID: u8 = 0x1B;

    /// Reads the packet body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let mut leash_buf = [0u8; 1];
        reader.read_exact(&mut leash_buf)?;
        let leash = leash_buf[0];
        let entity_id = read_i32(reader)?;
        let vehicle_id = read_i32(reader)?;
        Ok(Self {
            leash,
            entity_id,
            vehicle_id,
        })
    }
}

impl EncodablePacket for AttachEntity {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.leash])?;
        writer.write_all(&self.entity_id.to_be_bytes())?;
        writer.write_all(&self.vehicle_id.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_entity_roundtrip() {
        let packet = AttachEntity {
            leash: 0,
            entity_id: 10,
            vehicle_id: 20,
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = AttachEntity::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }
}
