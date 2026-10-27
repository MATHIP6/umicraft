// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Plugin Message` (0x3F, clientbound, play): custom payload on a channel.
//!
//! Layout (MCP `S3FPacketCustomPayload`, `readPacketData`): the channel name
//! (up to 20 chars) followed by the rest of the packet as the payload. The
//! reference client rejects payloads over 1048576 bytes; so does this
//! decoder, and it stops reading at that bound instead of buffering more.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_rest;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

const MAX_CHANNEL_CHARS: usize = 20;
/// Largest payload accepted, as in the reference client.
pub const MAX_PAYLOAD_BYTES: usize = 1_048_576;

/// `Plugin Message` (0x3F, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMessage {
    /// Channel name, e.g. `MC|Brand`.
    pub channel: String,
    /// Channel-specific payload (everything after the channel string).
    pub data: Vec<u8>,
}

impl PluginMessage {
    /// Wire packet id.
    pub const ID: u8 = 0x3F;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on a bad channel string or a payload over
    /// [`MAX_PAYLOAD_BYTES`].
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let channel = read_string(reader, MAX_CHANNEL_CHARS)?;
        // One byte past the limit is enough to detect an oversized payload.
        let data = read_rest(&mut reader.take(MAX_PAYLOAD_BYTES as u64 + 1))?;
        if data.len() > MAX_PAYLOAD_BYTES {
            return Err(ProtocolError::Invariant(format!(
                "plugin payload exceeds {MAX_PAYLOAD_BYTES} bytes"
            )));
        }
        Ok(Self { channel, data })
    }
}

impl EncodablePacket for PluginMessage {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.channel, MAX_CHANNEL_CHARS)?;
        writer.write_all(&self.data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_message_wire_shape() {
        let packet = PluginMessage {
            channel: "MC|Brand".to_string(),
            data: vec![7, b'v', b'a', b'n', b'i', b'l', b'l', b'a'],
        };
        let mut expected = vec![8];
        expected.extend(b"MC|Brand");
        expected.extend(&packet.data);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(PluginMessage::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_plugin_message_is_an_error() {
        assert!(PluginMessage::decode(&mut &[][..]).is_err());
        assert!(PluginMessage::decode(&mut &[3, b'a'][..]).is_err());
        // `read_string` budgets 4 bytes per char: 81 bytes is over it.
        let mut long_channel = vec![81];
        long_channel.extend([b'c'; 81]);
        assert!(PluginMessage::decode(&mut &long_channel[..]).is_err());
        let mut oversized = vec![1, b'c'];
        oversized.extend(vec![0u8; MAX_PAYLOAD_BYTES + 1]);
        assert!(PluginMessage::decode(&mut &oversized[..]).is_err());
        oversized.pop();
        assert!(PluginMessage::decode(&mut &oversized[..]).is_ok());
    }
}
