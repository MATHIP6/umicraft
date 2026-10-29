// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Sign Editor Open` (0x36, clientbound, play): the server asks the client
//! to open the sign editing screen.
//!
//! Layout (MCP `S36PacketSignEditorOpen`, `readPacketData`): the packed 1.8
//! block position of the sign (i64).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;

/// `Sign Editor Open` (0x36, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignEditorOpen {
    /// Block X of the sign.
    pub x: i32,
    /// Block Y of the sign.
    pub y: i32,
    /// Block Z of the sign.
    pub z: i32,
}

impl SignEditorOpen {
    /// Wire packet id.
    pub const ID: u8 = 0x36;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        Ok(Self { x, y, z })
    }
}

impl EncodablePacket for SignEditorOpen {
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
    fn sign_editor_open_wire_shape() {
        let expected = pack_block_position(-100, 5, 200).to_be_bytes();
        let packet = SignEditorOpen {
            x: -100,
            y: 5,
            z: 200,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(SignEditorOpen::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_position_is_an_error() {
        assert!(SignEditorOpen::decode(&mut &[0u8; 7][..]).is_err());
    }
}
