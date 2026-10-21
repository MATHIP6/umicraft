// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Login Success` (0x02, clientbound, login): authentication completed.
//!
//! Layout (`wiki.vg`, protocol 47): UUID as String (36-char cap) and the
//! profile username String (16-char cap).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// `Login Success` (0x02, clientbound, login): switches the client into the
/// play state with its canonical UUID and username.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSuccess {
    /// Canonical string form of the player UUID.
    pub uuid: String,
    /// Profile username used for the session.
    pub username: String,
}

impl LoginSuccess {
    /// Wire packet id.
    pub const ID: u8 = 0x02;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            uuid: read_string(reader, 36)?,
            username: read_string(reader, 16)?,
        })
    }
}

impl EncodablePacket for LoginSuccess {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.uuid, 36)?;
        write_string(writer, &self.username, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_success_parse() {
        let packet = LoginSuccess {
            uuid: "00000000-0000-0000-0000-000000000001".to_string(),
            username: "Player".to_string(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(LoginSuccess::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
