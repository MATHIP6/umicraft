// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Status Response` (0x00, clientbound): the server-list JSON payload.
//!
//! Layout (`wiki.vg`, protocol 47): one String (standard 256-char cap)
//! carrying the compact JSON document.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped, MAX_STRING_CHARS};

/// `Status Response` (0x00, clientbound): reply to `Status Request`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Raw JSON body (`version`, `players`, `description`, ...).
    pub json: String,
}

impl Response {
    /// Wire packet id.
    pub const ID: u8 = 0x00;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            json: read_string(reader, MAX_STRING_CHARS)?,
        })
    }
}

impl EncodablePacket for Response {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_round_trip() {
        let packet = Response {
            json: "{\"version\":{\"name\":\"1.8.8\",\"protocol\":47}}".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Response::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
