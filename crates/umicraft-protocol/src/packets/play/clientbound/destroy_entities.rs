// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Destroy Entities` (0x13, clientbound, play): the listed entities left
//! the world.
//!
//! Layout (MCP `S13PacketDestroyEntities`, `readPacketData`): a VarInt count,
//! then that many VarInt entity ids.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// Maximum number of entity ids accepted in one packet.
///
/// Every id costs at least one byte on the wire and a whole packet is
/// capped at 2 MiB, so a bigger count can only come from a hostile server.
const MAX_ENTITIES: usize = 4096;

/// `Destroy Entities` (0x13, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestroyEntities {
    /// The entity ids that were removed.
    pub entity_ids: Vec<i32>,
}

impl DestroyEntities {
    /// Wire packet id.
    pub const ID: u8 = 0x13;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, or a count above
    /// [`MAX_ENTITIES`].
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let count = read_var_i32(reader)?;
        if count < 0 {
            return Err(ProtocolError::Invariant(format!(
                "negative entity count {count}"
            )));
        }
        let count = count as usize;
        if count > MAX_ENTITIES {
            return Err(ProtocolError::Invariant(format!(
                "entity count {count} exceeds the budget of {MAX_ENTITIES}"
            )));
        }
        let mut entity_ids = Vec::with_capacity(count);
        for _ in 0..count {
            entity_ids.push(read_var_i32(reader)?);
        }
        Ok(Self { entity_ids })
    }
}

impl EncodablePacket for DestroyEntities {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_ids.len() as u32)?;
        for id in &self.entity_ids {
            write_var_u32(writer, *id as u32)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destroy_entities_wire_shape() {
        // Captured from a live 1.8 server: one entity id.
        let bytes = [0x01, 0xB0, 0xFD, 0x02];
        let packet = DestroyEntities::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_ids.len(), 1);
        assert_eq!(packet.entity_ids[0], 48_816);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn an_implausible_count_is_rejected_before_allocating() {
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        let err = DestroyEntities::decode(&mut &bytes[..]).unwrap_err();
        assert!(matches!(err, ProtocolError::Invariant(_)));
    }
}
