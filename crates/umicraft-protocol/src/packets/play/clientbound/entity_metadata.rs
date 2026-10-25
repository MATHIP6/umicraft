// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Metadata` (0x1C, clientbound, play): updates metadata / data watchers
//! on an existing entity.
//!
//! Layout (MCP `S1CPacketEntityMetadata`, `readPacketData`):
//!
//! | field | width |
//! |---|---|
//! | entity id | VarInt |
//! | metadata | data watcher list (see [`crate::packets::play::metadata`]) |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::metadata::Metadata;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Metadata` (0x1C, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    /// Entity ID.
    pub entity_id: i32,
    /// Data watcher list.
    pub metadata: Metadata,
}

impl EntityMetadata {
    /// Wire packet id.
    pub const ID: u8 = 0x1C;

    /// Reads the packet body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, malformed VarInt, or malformed metadata.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let metadata = Metadata::decode(reader)?;
        Ok(Self {
            entity_id,
            metadata,
        })
    }
}

impl EncodablePacket for EntityMetadata {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        self.metadata.encode(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_metadata_roundtrip() {
        let packet = EntityMetadata {
            entity_id: 42,
            metadata: Metadata::default(),
        };

        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        let decoded = EntityMetadata::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(packet, decoded);
    }
}
