// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tab-Complete` (0x14, serverbound, play): ask the server to complete the
//! text the player is typing.
//!
//! Layout (MCP `C14PacketTabComplete`, `writePacketData`): the text as a
//! string (cap 32767 chars), then a bool that says whether a looked-at block
//! follows, then that block as a packed 1.8 position (`i64`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_i64};
use crate::packets::play::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string, MAX_JSON_CHARS};

/// `Tab-Complete` (0x14, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabComplete {
    /// The text typed so far, including a leading slash for a command.
    pub message: String,
    /// The block the player looks at, used by commands that take coordinates.
    pub target_block: Option<(i32, i32, i32)>,
}

impl TabComplete {
    /// Wire packet id.
    pub const ID: u8 = 0x14;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let message = read_string(reader, MAX_JSON_CHARS)?;
        let target_block = if read_bool(reader)? {
            Some(unpack_block_position(read_i64(reader)?))
        } else {
            None
        };
        Ok(Self {
            message,
            target_block,
        })
    }
}

impl EncodablePacket for TabComplete {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.message, MAX_JSON_CHARS)?;
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
    fn a_request_without_a_target_round_trips() {
        let packet = TabComplete {
            message: "/gam".to_string(),
            target_block: None,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [4, b'/', b'g', b'a', b'm', 0]);
        assert_eq!(TabComplete::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn a_request_with_a_target_block_round_trips() {
        let packet = TabComplete {
            message: "/tp ".to_string(),
            target_block: Some((-12, 64, 300)),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(TabComplete::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
