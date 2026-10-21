// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Relative Move` (0x15, clientbound, play): an entity moved by a
//! small delta.
//!
//! Layout (MCP `S14PacketEntity.S15PacketEntityRelMove`,
//! `readPacketData`): VarInt entity id, then `i8` deltas for X, Y and Z
//! (in 1/32 of a block), and a boolean byte for "on ground".
//!
//! The deltas are relative to the last position the client knows, so they
//! accumulate: the reference client adds them to `serverPosX/Y/Z` and
//! divides by 32 (`NetHandlerPlayClient.handleEntityMovement`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_bool;
use crate::packets::play::wire::read_i8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Relative Move` (0x15, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityRelativeMove {
    /// The entity that moved.
    pub entity_id: i32,
    /// X delta in 1/32 of a block.
    pub dx: i8,
    /// Y delta in 1/32 of a block.
    pub dy: i8,
    /// Z delta in 1/32 of a block.
    pub dz: i8,
    /// True when the entity is standing on ground.
    pub on_ground: bool,
}

impl EntityRelativeMove {
    /// Wire packet id.
    pub const ID: u8 = 0x15;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            dx: read_i8(reader)?,
            dy: read_i8(reader)?,
            dz: read_i8(reader)?,
            on_ground: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for EntityRelativeMove {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.dx.to_be_bytes())?;
        writer.write_all(&self.dy.to_be_bytes())?;
        writer.write_all(&self.dz.to_be_bytes())?;
        writer.write_all(&[u8::from(self.on_ground)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_relative_move_wire_shape() {
        // Captured from a live 1.8 server: dx -6, dy 11, dz -2, on ground.
        let bytes = [0xAE, 0xBD, 0x01, 0xFA, 0x0B, 0xFE, 0x00];
        let packet = EntityRelativeMove::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);
        assert_eq!(packet.dx, -6);
        assert_eq!(packet.dy, 11);
        assert_eq!(packet.dz, -2);
        assert!(!packet.on_ground);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
