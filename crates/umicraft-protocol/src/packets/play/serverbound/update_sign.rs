// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Sign` (0x12, serverbound, play): the player finished editing a
//! sign.
//!
//! Layout (MCP `C12PacketUpdateSign`, `writePacketData`): one packed 1.8 block
//! position (i64), then four Strings holding each line as chat JSON (the
//! reference reads each with a 384-character cap).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// Character cap of one sign line (chat JSON), as in the reference reader.
const MAX_LINE_CHARS: usize = 384;

/// `Update Sign` (0x12, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSign {
    /// Block X of the sign.
    pub x: i32,
    /// Block Y of the sign.
    pub y: i32,
    /// Block Z of the sign.
    pub z: i32,
    /// The four lines, each a chat-component JSON document.
    pub lines: [String; 4],
}

impl UpdateSign {
    /// Wire packet id.
    pub const ID: u8 = 0x12;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8 or an over-long line.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(crate::packets::codec::read_i64(reader)?);
        let first = read_string(reader, MAX_LINE_CHARS)?;
        let second = read_string(reader, MAX_LINE_CHARS)?;
        let third = read_string(reader, MAX_LINE_CHARS)?;
        let fourth = read_string(reader, MAX_LINE_CHARS)?;
        Ok(Self {
            x,
            y,
            z,
            lines: [first, second, third, fourth],
        })
    }
}

impl EncodablePacket for UpdateSign {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        for line in &self.lines {
            write_string(writer, line, MAX_LINE_CHARS)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UpdateSign {
        UpdateSign {
            x: 10,
            y: 64,
            z: -3,
            lines: [
                "\"hi\"".to_string(),
                String::new(),
                "{\"text\":\"x\"}".to_string(),
                "\"\"".to_string(),
            ],
        }
    }

    #[test]
    fn update_sign_wire_shape() {
        let packet = sample();
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer[..8], pack_block_position(10, 64, -3).to_be_bytes());
        assert_eq!(buffer[8..12], [4, b'"', b'h', b'i']);
        assert_eq!(UpdateSign::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        let mut buffer = Vec::new();
        sample().encode_body(&mut buffer).unwrap();
        buffer.pop();
        assert!(UpdateSign::decode(&mut &buffer[..]).is_err());
    }
}
