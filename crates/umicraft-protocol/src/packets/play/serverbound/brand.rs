// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Plugin Message` (0x17, serverbound, play): the client brand channel.
//!
//! Layout pinned to MCP `C17PacketCustomPayload`: channel String (20-char
//! cap), then the payload is simply the rest of the body — 1.8 does not add
//! an inner length prefix (`readBytes(readableBytes)`). For `MC|Brand` the
//! outer payload itself is a VarInt-length-prefixed string built by the
//! caller.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_rest;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped};

/// `Plugin Message` (0x17, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brand {
    /// Channel name, normally `MC|Brand`.
    pub channel: String,
    /// Raw channel payload (everything after the channel string).
    pub data: Vec<u8>,
}

impl Brand {
    /// Wire packet id.
    pub const ID: u8 = 0x17;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8, or an over-long
    /// channel name.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            channel: read_string(reader, 20)?,
            data: read_rest(reader)?,
        })
    }
}

impl EncodablePacket for Brand {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.channel)?;
        // 1.8 sends the payload raw; MC|Brand's own content is a
        // length-prefixed string built by the caller.
        writer.write_all(&self.data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_wire_shape() {
        let packet = Brand {
            channel: "MC|Brand".to_string(),
            data: b"\x07umicraft".to_vec(),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        // Channel string, then the raw inner-length-prefixed brand payload.
        assert_eq!(buffer, b"\x08MC|Brand\x07umicraft");
        assert_eq!(Brand::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
