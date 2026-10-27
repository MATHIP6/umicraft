// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Update Sign` (0x33, clientbound, play): the text of a sign changed.
//!
//! Layout (MCP `S33PacketUpdateSign`, `readPacketData`): packed block
//! position (i64) and four chat components, each a JSON string read with
//! `readStringFromBuffer(32767)`. The JSON is kept verbatim; chat rendering
//! parses it later.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// Longest chat JSON string the reference client reads.
const MAX_LINE_CHARS: usize = 32767;

/// `Update Sign` (0x33, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSign {
    /// Block X of the sign.
    pub x: i32,
    /// Block Y of the sign.
    pub y: i32,
    /// Block Z of the sign.
    pub z: i32,
    /// The four lines as chat component JSON.
    pub lines: [String; 4],
}

impl UpdateSign {
    /// Wire packet id.
    pub const ID: u8 = 0x33;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an over-long or non-UTF-8 line.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let (x, y, z) = unpack_block_position(read_i64(reader)?);
        Ok(Self {
            x,
            y,
            z,
            lines: [
                read_string(reader, MAX_LINE_CHARS)?,
                read_string(reader, MAX_LINE_CHARS)?,
                read_string(reader, MAX_LINE_CHARS)?,
                read_string(reader, MAX_LINE_CHARS)?,
            ],
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

    #[test]
    fn update_sign_wire_shape() {
        let mut expected = pack_block_position(3, 64, -7).to_be_bytes().to_vec();
        expected.extend([0x02, b'"', b'a']); // line 1: "a (two bytes)
        expected.extend([0x00, 0x00, 0x00]); // three empty lines
        let packet = UpdateSign {
            x: 3,
            y: 64,
            z: -7,
            lines: ["\"a".into(), String::new(), String::new(), String::new()],
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(UpdateSign::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_sign_is_an_error() {
        assert!(UpdateSign::decode(&mut &[][..]).is_err());
        let mut short = pack_block_position(0, 0, 0).to_be_bytes().to_vec();
        short.extend([0x00, 0x00, 0x00]); // only three lines
        assert!(UpdateSign::decode(&mut &short[..]).is_err());
    }
}
