// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tab Complete` (0x14, serverbound, play): the player pressed tab in the
//! chat box.
//!
//! Layout (MCP `C14PacketTabComplete`, `writePacketData`): a String of up to
//! 32767 characters, a boolean, and, when the boolean is true, the packed 1.8
//! position (i64) of the block the player looks at.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_i64};
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// Character cap of the text, as in the reference.
const MAX_TEXT_CHARS: usize = 32_767;

/// `Tab Complete` (0x14, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabComplete {
    /// The text typed so far.
    pub text: String,
    /// The block the player looks at (X, Y, Z), if any.
    pub target_block: Option<(i32, i32, i32)>,
}

impl TabComplete {
    /// Wire packet id.
    pub const ID: u8 = 0x14;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, invalid UTF-8 or an over-long text.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let text = read_string(reader, MAX_TEXT_CHARS)?;
        let target_block = if read_bool(reader)? {
            Some(unpack_block_position(read_i64(reader)?))
        } else {
            None
        };
        Ok(Self { text, target_block })
    }
}

impl EncodablePacket for TabComplete {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.text, MAX_TEXT_CHARS)?;
        match self.target_block {
            Some((x, y, z)) => {
                writer.write_all(&[1])?;
                writer.write_all(&pack_block_position(x, y, z).to_be_bytes())?;
            }
            None => writer.write_all(&[0])?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_complete_without_a_target() {
        let bytes = [2, b'/', b'g', 0];
        let packet = TabComplete::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.text, "/g");
        assert_eq!(packet.target_block, None);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn tab_complete_with_a_target() {
        let packet = TabComplete {
            text: "/tp ".to_string(),
            target_block: Some((-5, 70, 12)),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer[5], 1);
        assert_eq!(buffer[6..], pack_block_position(-5, 70, 12).to_be_bytes());
        assert_eq!(TabComplete::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_target_is_rejected() {
        let bytes = [0, 1, 0, 0];
        assert!(TabComplete::decode(&mut &bytes[..]).is_err());
    }
}
