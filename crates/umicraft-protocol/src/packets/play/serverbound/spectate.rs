// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spectate` (0x18, serverbound, play): teleport to an entity while in
//! spectator mode.
//!
//! Layout (MCP `C18PacketSpectate`, `writePacketData`): the target's UUID as
//! two big-endian i64 (most significant half first), i.e. a big-endian u128.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;

/// `Spectate` (0x18, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spectate {
    /// UUID of the entity to spectate.
    pub uuid: u128,
}

impl Spectate {
    /// Wire packet id.
    pub const ID: u8 = 0x18;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let mut bytes = [0u8; 16];
        reader.read_exact(&mut bytes)?;
        Ok(Self {
            uuid: u128::from_be_bytes(bytes),
        })
    }
}

impl EncodablePacket for Spectate {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.uuid.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectate_wire_shape() {
        let uuid = 0x0011_2233_4455_6677_8899_AABB_CCDD_EEFF_u128;
        let bytes = uuid.to_be_bytes();
        let packet = Spectate::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.uuid, uuid);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        assert!(Spectate::decode(&mut &[0u8; 15][..]).is_err());
    }
}
