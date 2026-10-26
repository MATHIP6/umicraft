// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player List Header and Footer` (0x47, clientbound, play): the two chat
//! components shown above and below the tab list.
//!
//! Layout (MCP `S47PacketPlayerListHeaderFooter`, `readPacketData`): the
//! header component then the footer component, each a chat JSON string
//! (`readChatComponent`, cap 32767 chars). The wire id is the registration
//! index in `EnumConnectionState.PLAY` (0x47).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string, MAX_JSON_CHARS};

/// `Player List Header and Footer` (0x47, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerListHeaderFooter {
    /// The header as chat JSON.
    pub header: String,
    /// The footer as chat JSON.
    pub footer: String,
}

impl PlayerListHeaderFooter {
    /// Wire packet id.
    pub const ID: u8 = 0x47;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8 or an over-long string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            header: read_string(reader, MAX_JSON_CHARS)?,
            footer: read_string(reader, MAX_JSON_CHARS)?,
        })
    }
}

impl EncodablePacket for PlayerListHeaderFooter {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.header, MAX_JSON_CHARS)?;
        write_string(writer, &self.footer, MAX_JSON_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_footer_round_trip() {
        let packet = PlayerListHeaderFooter {
            header: r#"{"text":"top"}"#.to_string(),
            footer: r#"{"text":"bottom"}"#.to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(
            PlayerListHeaderFooter::decode(&mut &buffer[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn a_truncated_body_is_an_error_not_a_panic() {
        let packet = PlayerListHeaderFooter {
            header: "h".to_string(),
            footer: "f".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        buffer.pop();
        assert!(PlayerListHeaderFooter::decode(&mut &buffer[..]).is_err());
    }
}
