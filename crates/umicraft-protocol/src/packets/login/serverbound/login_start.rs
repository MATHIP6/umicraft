// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Login Start` (0x00, serverbound, login): the chosen username.
//!
//! Layout (`wiki.vg`, protocol 47): one String (16-char cap).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped};

/// `Login Start` (0x00, serverbound, login): first packet of the login
/// handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginStart {
    /// Profile username.
    pub username: String,
}

impl LoginStart {
    /// Wire packet id.
    pub const ID: u8 = 0x00;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            username: read_string(reader, 16)?,
        })
    }
}

impl EncodablePacket for LoginStart {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.username)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_start_round_trip() {
        let packet = LoginStart {
            username: "Alice".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(LoginStart::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
