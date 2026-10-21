// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Chat Message` (0x02, clientbound, play): chat and system messages.
//!
//! Layout (`wiki.vg`, protocol 47): chat JSON String (standard 256-char cap)
//! followed by a raw u8 position byte (1.8 reads it as an i8 filter: 0 chat,
//! 1 system, 2 action bar).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped, MAX_STRING_CHARS};

/// `Chat Message` (0x02, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// Raw chat JSON (`{"text": ...}` or component tree).
    pub json: String,
    /// System-bit field raw u8 (1.8 uses an i8 message filter).
    pub position: u8,
}

impl Chat {
    /// Wire packet id.
    pub const ID: u8 = 0x02;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            json: read_string(reader, MAX_STRING_CHARS)?,
            position: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for Chat {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.json)?;
        writer.write_all(&[self.position])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_round_trip() {
        let packet = Chat {
            json: "{\"text\":\"hi\"}".to_string(),
            position: 1,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Chat::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
