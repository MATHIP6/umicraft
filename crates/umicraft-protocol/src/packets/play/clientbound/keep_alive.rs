// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Keep Alive` (0x00, clientbound, play): liveness probe to answer.
//!
//! Layout (`wiki.vg` / MCP `S00PacketKeepAlive`): one VarInt payload. The
//! 64-bit keepalive id is a 1.12.2+ change, not 1.8.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Keep Alive` (0x00, clientbound, play): must be echoed with the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepAlive {
    /// Id as a VarInt int in 1.8 (`S00PacketKeepAlive` reads it via
    /// `readVarIntFromBuffer`).
    pub id: i32,
}

impl KeepAlive {
    /// Wire packet id.
    pub const ID: u8 = 0x00;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            id: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for KeepAlive {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.id as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_varint_id() {
        let bytes = [0xB2, 0xF2, 0x19];
        assert_eq!(
            KeepAlive::decode(&mut &bytes[..]).unwrap(),
            KeepAlive { id: 424_242 }
        );
    }
}
