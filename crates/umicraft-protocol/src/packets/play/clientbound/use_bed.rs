// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Use Bed` (0x0A, clientbound, play): a player went to sleep in a bed.
//!
//! Layout (MCP `S0APacketUseBed`, `readPacketData`): the player's entity id
//! (VarInt) and the packed 1.8 block position of the bed head (i64).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Use Bed` (0x0A, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UseBed {
    /// Entity id of the sleeping player.
    pub entity_id: i32,
    /// Block X of the bed.
    pub x: i32,
    /// Block Y of the bed.
    pub y: i32,
    /// Block Z of the bed.
    pub z: i32,
}

impl UseBed {
    /// Wire packet id.
    pub const ID: u8 = 0x0A;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        Ok(Self { entity_id, x, y, z })
    }
}

impl EncodablePacket for UseBed {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_bed_wire_shape() {
        let mut expected = vec![0x05]; // entity id 5
        expected.extend_from_slice(&pack_block_position(-4, 70, 12).to_be_bytes());
        let packet = UseBed {
            entity_id: 5,
            x: -4,
            y: 70,
            z: 12,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(UseBed::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_use_bed_is_an_error() {
        assert!(UseBed::decode(&mut &[][..]).is_err());
        assert!(UseBed::decode(&mut &[0x05, 0, 0][..]).is_err());
    }
}
