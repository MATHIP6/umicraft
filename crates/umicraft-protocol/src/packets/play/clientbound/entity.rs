// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity` (0x14, clientbound, play): the legacy entity packet.
//!
//! Layout (MCP `S14PacketEntity`, `readPacketData`): **only** a VarInt
//! entity id. The position and rotation fields of the class are never read
//! from the wire in 1.8 — the base class of `S15PacketEntityRelMove`,
//! `S16PacketEntityLook` and `S17PacketEntityLookMove` reads the id and
//! its subclasses add the movement fields.
//!
//! Vanilla does not use this packet to spawn entities in 1.8 (`Spawn Object`
//! 0x0E and `Spawn Mob` 0x0F do that); it is decoded here because the
//! server may still send it.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity` (0x14, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entity {
    /// The entity id.
    pub entity_id: i32,
}

impl Entity {
    /// Wire packet id.
    pub const ID: u8 = 0x14;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for Entity {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_carries_only_the_id() {
        let bytes = [0xAE, 0xBD, 0x01];
        let packet = Entity::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
