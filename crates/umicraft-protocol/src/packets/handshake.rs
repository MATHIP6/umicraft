// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Handshake` state: the single packet that opens every connection.
//!
//! Layout (`wiki.vg`, protocol 47): VarInt protocol version, String server
//! address (standard 256-char cap), u16 port, VarInt next state
//! (1 = status, 2 = login).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::types::{read_string_capped, write_string_capped};
use crate::varint::{read_var_i32, write_var_u32};

use super::EncodablePacket;

/// `Handshake` (0x00): first client packet, selects the next protocol state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    /// Intended protocol version; 1.8 clients send 47.
    pub protocol_version: i32,
    /// Server address as typed by the player (empty on dedicated proxies).
    pub server_address: String,
    /// Server port.
    pub server_port: u16,
    /// Next protocol state: 1 = status, 2 = login.
    pub next_state: i32,
}

impl Handshake {
    /// Wire packet id (the only packet in the handshake state).
    pub const ID: u8 = 0x00;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when `next_state` is not 1 or 2;
    /// [`ProtocolError`] on invalid fields or truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let protocol_version = read_var_i32(reader)?;
        let server_address = read_string_capped(reader)?;
        let mut port = [0u8; 2];
        reader.read_exact(&mut port)?;
        let next_state = read_var_i32(reader)?;
        if !matches!(next_state, 1 | 2) {
            return Err(ProtocolError::Invariant(format!(
                "handshake next_state {next_state}"
            )));
        }
        Ok(Self {
            protocol_version,
            server_address,
            server_port: u16::from_be_bytes(port),
            next_state,
        })
    }
}

impl EncodablePacket for Handshake {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.protocol_version as u32)?;
        write_string_capped(writer, &self.server_address)?;
        writer.write_all(&self.server_port.to_be_bytes())?;
        write_var_u32(writer, self.next_state as u32)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_round_trip() {
        let packet = Handshake {
            protocol_version: crate::state::PROTOCOL_VERSION_1_8,
            server_address: "localhost".to_string(),
            server_port: 25565,
            next_state: 2,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();

        let mut reader = &buffer[..];
        let decoded = Handshake::decode(&mut reader).unwrap();
        assert_eq!(decoded, packet);
        assert!(reader.is_empty());
        assert_eq!(packet.packet_id(), 0x00);
    }

    #[test]
    fn invalid_next_state_is_rejected() {
        let mut buffer = Vec::new();
        write_var_u32(&mut buffer, crate::state::PROTOCOL_VERSION_1_8 as u32).unwrap();
        write_string_capped(&mut buffer, "x").unwrap();
        buffer.extend_from_slice(&25565u16.to_be_bytes());
        write_var_u32(&mut buffer, 7).unwrap();
        assert!(Handshake::decode(&mut &buffer[..]).is_err());
    }
}
