// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Collect Item` (0x0D, clientbound, play): an entity picked up an item or
//! experience orb.
//!
//! Layout (MCP `S0DPacketCollectItem`, `readPacketData`): the collected
//! entity's id (VarInt) followed by the collector's id (VarInt).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Collect Item` (0x0D, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectItem {
    /// Entity id of the item (or orb) that was picked up.
    pub collected_entity_id: i32,
    /// Entity id of the collector.
    pub collector_entity_id: i32,
}

impl CollectItem {
    /// Wire packet id.
    pub const ID: u8 = 0x0D;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            collected_entity_id: read_var_i32(reader)?,
            collector_entity_id: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for CollectItem {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.collected_entity_id as u32)?;
        write_var_u32(writer, self.collector_entity_id as u32)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_item_wire_shape() {
        let expected = [0xAC, 0x02, 0x01]; // 300, 1
        let packet = CollectItem {
            collected_entity_id: 300,
            collector_entity_id: 1,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(CollectItem::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_collect_item_is_an_error() {
        assert!(CollectItem::decode(&mut &[0x01][..]).is_err());
        assert!(CollectItem::decode(&mut &[0x80][..]).is_err());
    }
}
