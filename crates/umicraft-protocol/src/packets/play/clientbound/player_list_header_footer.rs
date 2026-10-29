// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player List Header/Footer` (0x47, clientbound, play): text above and
//! below the tab list.
//!
//! Layout (MCP `S47PacketPlayerListHeaderFooter`, `readPacketData`): two chat
//! components, each a JSON string of up to 32767 chars (header, then footer).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

const MAX_CHAT_CHARS: usize = 32767;

/// `Player List Header/Footer` (0x47, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerListHeaderFooter {
    /// Header chat JSON.
    pub header: String,
    /// Footer chat JSON.
    pub footer: String,
}

impl PlayerListHeaderFooter {
    /// Wire packet id.
    pub const ID: u8 = 0x47;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            header: read_string(reader, MAX_CHAT_CHARS)?,
            footer: read_string(reader, MAX_CHAT_CHARS)?,
        })
    }
}

impl EncodablePacket for PlayerListHeaderFooter {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.header, MAX_CHAT_CHARS)?;
        write_string(writer, &self.footer, MAX_CHAT_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_footer_wire_shape() {
        let packet = PlayerListHeaderFooter {
            header: "{}".to_string(),
            footer: "[]".to_string(),
        };
        let expected = [2, b'{', b'}', 2, b'[', b']'];
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            PlayerListHeaderFooter::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn hostile_header_footer_is_an_error() {
        assert!(PlayerListHeaderFooter::decode(&mut &[][..]).is_err());
        assert!(PlayerListHeaderFooter::decode(&mut &[2, b'{', b'}'][..]).is_err());
    }
}
