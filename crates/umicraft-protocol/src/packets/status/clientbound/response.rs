// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Status Response` (0x00, clientbound): the server-list JSON payload.
//!
//! Layout (`wiki.vg`, protocol 47): one String carrying the compact JSON
//! document. The usual 256-char cap does not apply: a favicon alone is several
//! thousand characters of base64, and the reference client reads this string
//! with the 32 767-char protocol maximum (`S00PacketServerInfo`). Umicraft
//! accepts [`MAX_JSON_CHARS`] so an oversized favicon can be dropped by the
//! caller instead of failing the whole ping.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// Longest accepted JSON body, in characters.
pub const MAX_JSON_CHARS: usize = 131_072;

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
            json: read_string(reader, MAX_JSON_CHARS)?,
        })
    }
}

impl EncodablePacket for Response {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.json, MAX_JSON_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_with_a_favicon_sized_body_round_trips() {
        let packet = Response {
            json: "x".repeat(10_000),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Response::decode(&mut &buffer[..]).unwrap(), packet);
    }

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
