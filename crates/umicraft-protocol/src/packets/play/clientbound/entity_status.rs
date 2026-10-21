// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Status` (0x1A, clientbound, play): an entity triggered a status
//! event (hurt, death, …).
//!
//! Layout (MCP `S19PacketEntityStatus`, `readPacketData`): a **plain** `i32`
//! entity id (not a VarInt) followed by a signed byte with the status code.
//!
//! The wire id of this packet is 0x1A even though the class is called
//! `S19…`: the id is the registration index in `EnumConnectionState`, and
//! `S19PacketEntityHeadLook` took 0x19 (ADR-0005 §A.4).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::play::wire::read_i8;
use crate::packets::EncodablePacket;

/// `Entity Status` (0x1A, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityStatus {
    /// The entity the status belongs to.
    pub entity_id: i32,
    /// Status code (2 = hurt, 3 = death, 4 = iron golem attack, …).
    pub status: i8,
}

impl EntityStatus {
    /// Wire packet id.
    pub const ID: u8 = 0x1A;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_i32(reader)?,
            status: read_i8(reader)?,
        })
    }
}

impl EncodablePacket for EntityStatus {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.entity_id.to_be_bytes())?;
        writer.write_all(&self.status.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_status_wire_shape() {
        let bytes = [0x00, 0x00, 0x01, 0x02, 0x02];
        let packet = EntityStatus::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 258);
        assert_eq!(packet.status, 2);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn the_status_packet_keeps_its_registration_index() {
        assert_eq!(EntityStatus::ID, 0x1A);
    }
}
