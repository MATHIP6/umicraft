// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Entity NBT` (0x49, clientbound, play): NBT data of an entity.
//!
//! Layout (MCP `S49PacketUpdateEntityNBT`, `readPacketData`): entity id
//! (VarInt) and an optional NBT compound (`readNBTTagCompoundFromBuffer`, a
//! single `0x00` when absent). The compound is validated and kept as raw
//! bytes.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::nbt::read_optional_nbt;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Update Entity NBT` (0x49, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateEntityNbt {
    /// Entity the data belongs to.
    pub entity_id: i32,
    /// Raw NBT compound bytes, or `None` when the server sent none.
    pub nbt: Option<Vec<u8>>,
}

impl UpdateEntityNbt {
    /// Wire packet id.
    pub const ID: u8 = 0x49;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt or malformed NBT.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            nbt: read_optional_nbt(reader)?,
        })
    }
}

impl EncodablePacket for UpdateEntityNbt {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        match &self.nbt {
            Some(bytes) => writer.write_all(bytes)?,
            None => writer.write_all(&[0x00])?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_entity_nbt_wire_shape() {
        let expected = [0x05, 0x0A, 0x00, 0x00, 0x00];
        let packet = UpdateEntityNbt {
            entity_id: 5,
            nbt: Some(vec![0x0A, 0x00, 0x00, 0x00]),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(UpdateEntityNbt::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn absent_nbt_round_trips_and_truncation_fails() {
        let packet = UpdateEntityNbt {
            entity_id: 1,
            nbt: None,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [0x01, 0x00]);
        assert_eq!(UpdateEntityNbt::decode(&mut &buffer[..]).unwrap(), packet);
        assert!(UpdateEntityNbt::decode(&mut &[0x01][..]).is_err());
        assert!(UpdateEntityNbt::decode(&mut &[0x01, 0x0A, 0x00][..]).is_err());
    }
}
