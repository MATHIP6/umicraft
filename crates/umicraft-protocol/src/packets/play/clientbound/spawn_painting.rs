// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Painting` (0x10, clientbound, play): a painting entity appeared.
//!
//! Layout (MCP `S10PacketSpawnPainting`, `readPacketData`): entity id
//! (VarInt), art title (string, at most 13 chars — the length of
//! `"SkullAndRoses"`, the longest title in `EntityPainting.EnumArt`), packed
//! block position (i64) and the horizontal facing as an unsigned byte
//! (`EnumFacing.getHorizontal`: 0 south, 1 west, 2 north, 3 east).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_i64, read_u8};
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

/// Longest painting title the reference client accepts.
const MAX_TITLE_CHARS: usize = 13;

/// `Spawn Painting` (0x10, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnPainting {
    /// Entity id of the painting.
    pub entity_id: i32,
    /// Name of the artwork (for example `"Kebab"`).
    pub title: String,
    /// Block X of the painting.
    pub x: i32,
    /// Block Y of the painting.
    pub y: i32,
    /// Block Z of the painting.
    pub z: i32,
    /// Horizontal facing index (0 south, 1 west, 2 north, 3 east).
    pub facing: u8,
}

impl SpawnPainting {
    /// Wire packet id.
    pub const ID: u8 = 0x10;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, an over-long
    /// title, or a facing byte above 3 (the reference indexes a four-entry
    /// table with it).
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let title = read_string(reader, MAX_TITLE_CHARS)?;
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        let facing = read_u8(reader)?;
        if facing > 3 {
            return Err(ProtocolError::Invariant(format!(
                "painting facing {facing} is not a horizontal index"
            )));
        }
        Ok(Self {
            entity_id,
            title,
            x,
            y,
            z,
            facing,
        })
    }
}

impl EncodablePacket for SpawnPainting {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        write_string(writer, &self.title, MAX_TITLE_CHARS)?;
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.facing])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_painting_wire_shape() {
        let mut expected = vec![0x07, 0x05];
        expected.extend_from_slice(b"Kebab");
        expected.extend_from_slice(&pack_block_position(1, 2, 3).to_be_bytes());
        expected.push(2);
        let packet = SpawnPainting {
            entity_id: 7,
            title: "Kebab".into(),
            x: 1,
            y: 2,
            z: 3,
            facing: 2,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(SpawnPainting::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_paintings_are_errors() {
        assert!(SpawnPainting::decode(&mut &[][..]).is_err());
        // Title length 200 exceeds the 13 char budget.
        assert!(SpawnPainting::decode(&mut &[0x01, 0xC8, 0x01, b'a'][..]).is_err());
        let mut bad_facing = vec![0x01, 0x01, b'a'];
        bad_facing.extend_from_slice(&[0; 8]);
        bad_facing.push(9);
        assert!(SpawnPainting::decode(&mut &bad_facing[..]).is_err());
    }
}
