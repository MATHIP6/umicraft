// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Chat Message` (0x01, serverbound, play): player-typed chat.
//!
//! Layout (`wiki.vg`, protocol 47): one String capped at 100 characters in
//! 1.8 servers (`C01PacketChatMessage` clamps and rejects longer input); the
//! standard 256-char read cap is kept here as the wire-level sanity bound.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped, MAX_STRING_CHARS};

/// `Chat Message` (0x01, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// Raw text; a leading slash marks a command.
    pub message: String,
}

impl Chat {
    /// Wire packet id.
    pub const ID: u8 = 0x01;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            message: read_string(reader, MAX_STRING_CHARS)?,
        })
    }
}

impl EncodablePacket for Chat {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_round_trip() {
        let packet = Chat {
            message: "/help".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Chat::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
