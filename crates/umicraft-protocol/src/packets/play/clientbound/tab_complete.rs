// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tab-Complete` (0x3A, clientbound, play): completion candidates.
//!
//! Layout (MCP `S3APacketTabComplete`, `readPacketData`): a VarInt count and
//! that many strings (up to 32767 chars each).

use std::io::{Read, Write};

use super::statistics::read_list_len;
use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::write_var_u32;

const MAX_MATCH_CHARS: usize = 32767;

/// `Tab-Complete` (0x3A, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabComplete {
    /// Completion candidates in wire order.
    pub matches: Vec<String>,
}

impl TabComplete {
    /// Wire packet id.
    pub const ID: u8 = 0x3A;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a bad count or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let len = read_list_len(reader)?;
        let mut matches = Vec::new();
        for _ in 0..len {
            matches.push(read_string(reader, MAX_MATCH_CHARS)?);
        }
        Ok(Self { matches })
    }
}

impl EncodablePacket for TabComplete {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.matches.len() as u32)?;
        for candidate in &self.matches {
            write_string(writer, candidate, MAX_MATCH_CHARS)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_complete_wire_shape() {
        let packet = TabComplete {
            matches: vec!["/gamemode".to_string(), "/give".to_string()],
        };
        let mut expected = vec![2, 9];
        expected.extend(b"/gamemode");
        expected.push(5);
        expected.extend(b"/give");
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(TabComplete::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_tab_complete_is_an_error() {
        assert!(TabComplete::decode(&mut &[][..]).is_err());
        assert!(TabComplete::decode(&mut &[1, 5, b'a'][..]).is_err());
        assert!(TabComplete::decode(&mut &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F][..]).is_err());
    }
}
