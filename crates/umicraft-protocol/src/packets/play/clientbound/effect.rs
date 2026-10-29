// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Effect` (0x28, clientbound, play): a world event such as a door sound,
//! a record playing or a block-break particle burst.
//!
//! Layout (MCP `S28PacketEffect`, `readPacketData`): effect id (i32), packed
//! block position (i64), effect-specific data (i32) and a "disable relative
//! volume" boolean (the reference calls it `serverWide`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_i32, read_i64};
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;

/// `Effect` (0x28, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// World event id (1000 click, 1005 record, 2001 block break, ...).
    pub effect_id: i32,
    /// Block X of the event.
    pub x: i32,
    /// Block Y of the event.
    pub y: i32,
    /// Block Z of the event.
    pub z: i32,
    /// Event-specific data (record item id, block state, ...).
    pub data: i32,
    /// True when the sound ignores distance attenuation.
    pub disable_relative_volume: bool,
}

impl Effect {
    /// Wire packet id.
    pub const ID: u8 = 0x28;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an invalid boolean.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let effect_id = read_i32(reader)?;
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        Ok(Self {
            effect_id,
            x,
            y,
            z,
            data: read_i32(reader)?,
            disable_relative_volume: read_bool(reader)?,
        })
    }
}

impl EncodablePacket for Effect {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.effect_id.to_be_bytes())?;
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&self.data.to_be_bytes())?;
        writer.write_all(&[u8::from(self.disable_relative_volume)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_wire_shape() {
        let mut expected = Vec::new();
        expected.extend_from_slice(&2001i32.to_be_bytes());
        expected.extend_from_slice(&pack_block_position(8, 63, -8).to_be_bytes());
        expected.extend_from_slice(&1i32.to_be_bytes());
        expected.push(1);
        let packet = Effect {
            effect_id: 2001,
            x: 8,
            y: 63,
            z: -8,
            data: 1,
            disable_relative_volume: true,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Effect::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_effect_is_an_error() {
        assert!(Effect::decode(&mut &[][..]).is_err());
        assert!(Effect::decode(&mut &[0u8; 16][..]).is_err());
    }
}
