// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Client Settings` (0x15, serverbound, play): locale, view distance and
//! skin flags.
//!
//! Layout pinned to MCP `C15PacketClientSettings`: locale String (server
//! reads at most 7 chars in 1.8), u8 view distance, a **byte** chat-visibility
//! (not a VarInt), `chat_colors` bool, raw u8 skin-parts mask.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_u8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string_capped};

/// `Client Settings` (0x15, serverbound, play): required before some servers
/// release world content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSettings {
    /// e.g. `en_us` (server reads at most 7 chars in 1.8).
    pub locale: String,
    /// Render distance the client accepts (chunks).
    pub view_distance: u8,
    /// Raw chat-visibility byte (`C15PacketClientSettings` reads a byte).
    pub chat_mode: u8,
    /// Whether colors are enabled in chat.
    pub chat_colors: bool,
    /// Raw displayed-skin-part mask.
    pub skin_parts: u8,
}

impl ClientSettings {
    /// Wire packet id.
    pub const ID: u8 = 0x15;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8, or an over-long locale.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            locale: read_string(reader, 7)?,
            view_distance: read_u8(reader)?,
            chat_mode: read_u8(reader)?,
            chat_colors: read_bool(reader)?,
            skin_parts: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for ClientSettings {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string_capped(writer, &self.locale)?;
        writer.write_all(&[self.view_distance, self.chat_mode])?;
        writer.write_all(&[u8::from(self.chat_colors), self.skin_parts])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_settings_wire_shape() {
        let packet = ClientSettings {
            locale: "en_us".to_string(),
            view_distance: 8,
            chat_mode: 0,
            chat_colors: true,
            skin_parts: 0b0111_1111,
        };
        let mut expected = Vec::new();
        expected.extend([5]); // VarInt string length
        expected.extend(b"en_us");
        expected.extend([8, 0, 1, 0b0111_1111]);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(ClientSettings::decode(&mut &expected[..]).unwrap(), packet);
    }
}
