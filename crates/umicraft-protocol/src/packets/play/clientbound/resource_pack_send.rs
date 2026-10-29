// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Resource Pack Send` (0x48, clientbound, play): offers a resource pack.
//!
//! Layout (MCP `S48PacketResourcePackSend`, `readPacketData`): the pack URL
//! (up to 32767 chars) and its SHA-1 hash string (up to 40 chars).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

const MAX_URL_CHARS: usize = 32767;
const MAX_HASH_CHARS: usize = 40;

/// `Resource Pack Send` (0x48, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePackSend {
    /// Download URL (`http`, `https` or `level://`).
    pub url: String,
    /// Lowercase hex SHA-1 of the pack; may be empty.
    pub hash: String,
}

impl ResourcePackSend {
    /// Wire packet id.
    pub const ID: u8 = 0x48;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            url: read_string(reader, MAX_URL_CHARS)?,
            hash: read_string(reader, MAX_HASH_CHARS)?,
        })
    }
}

impl EncodablePacket for ResourcePackSend {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.url, MAX_URL_CHARS)?;
        write_string(writer, &self.hash, MAX_HASH_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_pack_send_wire_shape() {
        let packet = ResourcePackSend {
            url: "http://a".to_string(),
            hash: "ff".to_string(),
        };
        let mut expected = vec![8];
        expected.extend(b"http://a");
        expected.extend([2, b'f', b'f']);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            ResourcePackSend::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn hostile_resource_pack_send_is_an_error() {
        assert!(ResourcePackSend::decode(&mut &[][..]).is_err());
        assert!(ResourcePackSend::decode(&mut &[1, b'a'][..]).is_err());
        // `read_string` budgets 4 bytes per char: 161 bytes is over it.
        let mut long_hash = vec![1, b'a', 0xA1, 0x01];
        long_hash.extend([b'0'; 161]);
        assert!(ResourcePackSend::decode(&mut &long_hash[..]).is_err());
    }
}
