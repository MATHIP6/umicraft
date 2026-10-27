// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Spawn Experience Orb` (0x11, clientbound, play): an XP orb appeared.
//!
//! Layout (MCP `S11PacketSpawnExperienceOrb`, `readPacketData`): entity id
//! (VarInt), x/y/z as fixed-point i32 (coordinate * 32) and the experience
//! amount as an i16.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::play::wire::read_i16;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Spawn Experience Orb` (0x11, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnXpOrb {
    /// Entity id of the orb.
    pub entity_id: i32,
    /// X position, fixed-point (coordinate * 32).
    pub x: i32,
    /// Y position, fixed-point (coordinate * 32).
    pub y: i32,
    /// Z position, fixed-point (coordinate * 32).
    pub z: i32,
    /// Experience points the orb is worth.
    pub count: i16,
}

impl SpawnXpOrb {
    /// Wire packet id.
    pub const ID: u8 = 0x11;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            x: read_i32(reader)?,
            y: read_i32(reader)?,
            z: read_i32(reader)?,
            count: read_i16(reader)?,
        })
    }
}

impl EncodablePacket for SpawnXpOrb {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&self.count.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_xp_orb_wire_shape() {
        let mut expected = vec![0x09];
        expected.extend_from_slice(&32i32.to_be_bytes());
        expected.extend_from_slice(&(-64i32).to_be_bytes());
        expected.extend_from_slice(&96i32.to_be_bytes());
        expected.extend_from_slice(&[0x00, 0x11]);
        let packet = SpawnXpOrb {
            entity_id: 9,
            x: 32,
            y: -64,
            z: 96,
            count: 17,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(SpawnXpOrb::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_orb_is_an_error() {
        assert!(SpawnXpOrb::decode(&mut &[][..]).is_err());
        assert!(SpawnXpOrb::decode(&mut &[0x09, 0, 0, 0, 1][..]).is_err());
    }
}
