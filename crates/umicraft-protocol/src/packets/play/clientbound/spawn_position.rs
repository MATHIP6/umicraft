// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Position` (0x05, clientbound, play): the world spawn point.
//!
//! Layout (MCP `S05PacketSpawnPosition`, `readPacketData`): one packed 1.8
//! block position (`BlockPos.toLong`, 26-bit X at bit 38, 12-bit Y at bit 26,
//! 26-bit Z at bit 0).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;

/// `Spawn Position` (0x05, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnPosition {
    /// Block X of the spawn point.
    pub x: i32,
    /// Block Y of the spawn point.
    pub y: i32,
    /// Block Z of the spawn point.
    pub z: i32,
}

impl SpawnPosition {
    /// Wire packet id.
    pub const ID: u8 = 0x05;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        Ok(Self { x, y, z })
    }
}

impl EncodablePacket for SpawnPosition {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_position_wire_shape() {
        // Captured from a live 1.8 server: 8 body bytes, one packed position.
        let bytes = [0xFF, 0xFE, 0x55, 0x40, 0x10, 0x00, 0x00, 0x67];
        let packet = SpawnPosition::decode(&mut &bytes[..]).unwrap();
        assert_eq!(
            packet,
            SpawnPosition {
                x: -1707,
                y: 4,
                z: 103
            }
        );

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }
}
