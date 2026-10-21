// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Encryption Request` (0x01, clientbound, login): online-mode handshake
//! challenge.
//!
//! Layout (`wiki.vg` / MCP `S01PacketEncryptionRequest`): server id String
//! (server write cap 20), public key byte array (VarInt length, budget
//! 8192), verify token byte array (VarInt length, budget 128).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_byte_array;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::write_var_u32;

/// `Encryption Request` (0x01, clientbound, login): signals an online-mode
/// server and hands out its RSA key plus the verify token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionRequest {
    /// Usually empty since 1.7; authenticates the request token narrative.
    pub server_id: String,
    /// DER-encoded X.509 SubjectPublicKeyInfo of the server's RSA key.
    pub public_key: Vec<u8>,
    /// Verify token to be returned unmodified after encryption.
    pub verify_token: Vec<u8>,
}

impl EncryptionRequest {
    /// Wire packet id.
    pub const ID: u8 = 0x01;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] on implausible array lengths,
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            server_id: read_string(reader, 20)?,
            public_key: read_byte_array(reader, 8192)?,
            verify_token: read_byte_array(reader, 128)?,
        })
    }
}

impl EncodablePacket for EncryptionRequest {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.server_id, 20)?;
        write_var_u32(writer, self.public_key.len() as u32)?;
        writer.write_all(&self.public_key)?;
        write_var_u32(writer, self.verify_token.len() as u32)?;
        writer.write_all(&self.verify_token)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_request_round_trip() {
        let packet = EncryptionRequest {
            server_id: String::new(),
            public_key: vec![0xAB; 32],
            verify_token: vec![0xCD; 16],
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(EncryptionRequest::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
