// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Keep Alive` (0x00, serverbound, play): the client's echo.
//!
//! Layout (`wiki.vg` / MCP `C00PacketKeepAlive`): one VarInt id copied from
//! the server's probe (`readVarIntFromBuffer`).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Keep Alive` (0x00, serverbound, play): echo of the server's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepAlive {
    /// Id copied from the server's packet (VarInt in 1.8).
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
    fn keepalive_ids_are_varints_in_1_8() {
        let packet = KeepAlive { id: 424_242 };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        // 424_242 takes 3 VarInt bytes, not the 8 a 1.12.2 i64 id would.
        assert_eq!(buffer, [0xB2, 0xF2, 0x19]);
        assert_eq!(KeepAlive::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
