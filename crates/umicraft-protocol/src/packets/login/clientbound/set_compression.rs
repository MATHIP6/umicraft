// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Set Compression` (0x03, clientbound, login): enables zlib framing from
//! the packet *after* this one.
//!
//! Layout (`wiki.vg` / MCP `S03PacketEnableCompression`): VarInt threshold;
//! 0 or negative disables compression entirely (vanilla sends a huge value
//! to disable, we keep the raw threshold).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Set Compression` (0x03, clientbound, login).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetCompression {
    /// Threshold in bytes; 0 disables compression entirely.
    pub threshold: i32,
}

impl SetCompression {
    /// Wire packet id.
    pub const ID: u8 = 0x03;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or malformed VarInt.
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
    fn set_compression_parse() {
        let mut buffer = Vec::new();
        write_var_u32(&mut buffer, 256).unwrap();
        assert_eq!(
            SetCompression::decode(&mut &buffer[..]).unwrap(),
            SetCompression { threshold: 256 }
        );
    }
}
