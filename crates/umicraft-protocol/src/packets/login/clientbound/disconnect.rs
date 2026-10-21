// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Disconnect` (0x00, clientbound, login state): kick reason as chat JSON.
//!
//! Layout (`wiki.vg`, protocol 47): one String (standard 256-char cap)
//! carrying a chat component.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped, MAX_STRING_CHARS};

/// `Disconnect` (0x00, clientbound, login): the server refused the login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disconnect {
    /// Chat JSON body of the kick reason.
    pub reason: String,
}

impl Disconnect {
    /// Wire packet id.
    pub const ID: u8 = 0x00;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            reason: read_string(reader, MAX_STRING_CHARS)?,
        })
    }
}

impl EncodablePacket for Disconnect {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_round_trip() {
        let packet = Disconnect {
            reason: "{\"text\":\"bye\"}".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Disconnect::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
