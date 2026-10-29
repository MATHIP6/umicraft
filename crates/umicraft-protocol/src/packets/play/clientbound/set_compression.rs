// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Set Compression` (0x46, clientbound, play): changes the compression
//! threshold mid-session.
//!
//! Layout (MCP `S46PacketSetCompressionLevel`, `readPacketData`): the
//! threshold as a VarInt; a negative value disables compression.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Set Compression` (0x46, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetCompression {
    /// Smallest packet (bytes) that is compressed; negative disables it.
    pub threshold: i32,
}

impl SetCompression {
    /// Wire packet id.
    pub const ID: u8 = 0x46;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            threshold: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for SetCompression {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.threshold as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_compression_wire_shape() {
        let mut buffer = Vec::new();
        SetCompression { threshold: 256 }
            .encode_body(&mut buffer)
            .unwrap();
        assert_eq!(buffer, [0x80, 0x02]);
        assert_eq!(
            SetCompression::decode(&mut &buffer[..]).unwrap().threshold,
            256
        );
        let mut negative = Vec::new();
        SetCompression { threshold: -1 }
            .encode_body(&mut negative)
            .unwrap();
        assert_eq!(
            SetCompression::decode(&mut &negative[..])
                .unwrap()
                .threshold,
            -1
        );
    }

    #[test]
    fn hostile_set_compression_is_an_error() {
        assert!(SetCompression::decode(&mut &[][..]).is_err());
        assert!(SetCompression::decode(&mut &[0xFF; 6][..]).is_err());
    }
}
