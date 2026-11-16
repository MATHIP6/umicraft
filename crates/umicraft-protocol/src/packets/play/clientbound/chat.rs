// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Chat Message` (0x02, clientbound, play): chat and system messages.
//!
//! Layout (`wiki.vg`, protocol 47, MCP `S02PacketChat`): chat JSON String
//! (capped at 32767 chars / `MAX_JSON_CHARS`, read via `readChatComponent`)
//! followed by a raw u8 position byte (1.8 reads it as an i8 filter: 0 chat,
//! 1 system, 2 action bar).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string, MAX_JSON_CHARS};

/// `Chat Message` (0x02, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// Raw chat JSON (`{"text": ...}` or component tree).
    pub json: String,
    /// System-bit field raw u8 (1.8 uses an i8 message filter).
    pub position: u8,
}

impl Chat {
    /// Wire packet id.
    pub const ID: u8 = 0x02;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or invalid UTF-8.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            json: read_string(reader, MAX_JSON_CHARS)?,
            position: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for Chat {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.json, MAX_JSON_CHARS)?;
        writer.write_all(&[self.position])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_round_trip() {
        let packet = Chat {
            json: "{\"text\":\"hi\"}".to_string(),
            position: 1,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Chat::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn chat_decodes_minemen_1450_bytes_payload() {
        // Minemen match start packet regression test: 1450 bytes chat JSON
        let json_body = format!(
            r#"{{"text":"","extra":[{{"text":"[MinemenClub] Match Starting vs ","color":"gold"}},{{"text":"{}"}}]}}"#,
            "X".repeat(1358)
        );
        assert_eq!(json_body.len(), 1450);

        let packet = Chat {
            json: json_body,
            position: 1,
        };

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        // Body length on wire: VarInt length prefix + 1450 bytes JSON + 1 byte position
        let decoded = Chat::decode(&mut &buffer[..]).expect("1450-byte Minemen chat must decode successfully");
        assert_eq!(decoded, packet);
    }

    #[test]
    fn chat_decodes_minemen_1077_bytes_payload() {
        // Minemen match info packet regression test: 1077 bytes chat JSON
        let json_body = format!(
            r#"{{"text":"","extra":[{{"text":"Rules and Map: ","color":"yellow"}},{{"text":"{}"}}]}}"#,
            "Y".repeat(1000)
        );
        assert_eq!(json_body.len(), 1077);

        let packet = Chat {
            json: json_body,
            position: 0,
        };

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        let decoded = Chat::decode(&mut &buffer[..]).expect("1077-byte Minemen chat must decode successfully");
        assert_eq!(decoded, packet);
    }

    #[test]
    fn chat_handles_rich_json_with_unicode_and_emojis() {
        let json_body = r#"{"text":"Victory! ⚔️ \u{1F3C6}","bold":true,"color":"green","hoverEvent":{"action":"show_text","value":{"text":"Score: 100 \u{1F525}"}}}"#;
        let packet = Chat {
            json: json_body.to_string(),
            position: 2,
        };

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        let decoded = Chat::decode(&mut &buffer[..]).unwrap();
        assert_eq!(decoded, packet);
    }
}
