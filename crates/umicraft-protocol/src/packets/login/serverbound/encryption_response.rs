// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Encryption Response` (0x01, serverbound, login): RSA-wrapped shared
//! secret and verify token.
//!
//! Layout (`wiki.vg` / MCP `C01PacketEncryptionResponse`): two byte arrays
//! (VarInt length each) — encrypted shared secret and encrypted verify
//! token. Vanilla sizes are 128 and 16 bytes; the read budget is 256 for
//! both so slightly larger keys stay acceptable.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_byte_array;
use crate::packets::EncodablePacket;
use crate::varint::write_var_u32;

/// `Encryption Response` (0x01, serverbound, login): completes the
/// online-mode key exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionResponse {
    /// Shared secret encrypted with the server's public key (128 bytes).
    pub shared_secret: Vec<u8>,
    /// Verify token encrypted with the same key (16 bytes vanilla).
    pub nonce: Vec<u8>,
}

impl EncryptionResponse {
    /// Wire packet id.
    pub const ID: u8 = 0x01;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] on implausible array lengths,
    /// [`ProtocolError`] on truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            shared_secret: read_byte_array(reader, 256)?,
            nonce: read_byte_array(reader, 256)?,
        })
    }
}

impl EncodablePacket for EncryptionResponse {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.shared_secret.len() as u32)?;
        writer.write_all(&self.shared_secret)?;
        write_var_u32(writer, self.nonce.len() as u32)?;
        writer.write_all(&self.nonce)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_response_round_trip() {
        let packet = EncryptionResponse {
            shared_secret: vec![9u8; 16],
            nonce: vec![7u8; 128],
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(
            EncryptionResponse::decode(&mut &buffer[..]).unwrap(),
            packet
        );
    }
}
