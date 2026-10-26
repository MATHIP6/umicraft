// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tab-Complete` (0x3A, clientbound, play): the completions the server
//! offers for the text the player is typing.
//!
//! Layout (MCP `S3APacketTabComplete`, `readPacketData`): a VarInt count, then
//! that many strings (cap 32767 chars each).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string, MAX_JSON_CHARS};
use crate::varint::{read_var_i32, write_var_u32};

/// Maximum number of completions accepted in one packet.
///
/// Every string costs at least one byte and a whole packet is capped at
/// 2 MiB, so a bigger count can only come from a hostile server.
const MAX_MATCHES: usize = 4096;

/// `Tab-Complete` (0x3A, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabComplete {
    /// The suggested completions, in server order.
    pub matches: Vec<String>,
}

impl TabComplete {
    /// Wire packet id.
    pub const ID: u8 = 0x3A;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a negative or oversized count, or an
    /// invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let count = read_var_i32(reader)?;
        let count = usize::try_from(count)
            .ok()
            .filter(|count| *count <= MAX_MATCHES)
            .ok_or_else(|| {
                ProtocolError::Invariant(format!("completion count {count} is out of range"))
            })?;
        let mut matches = Vec::with_capacity(count);
        for _ in 0..count {
            matches.push(read_string(reader, MAX_JSON_CHARS)?);
        }
        Ok(Self { matches })
    }
}

impl EncodablePacket for TabComplete {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.matches.len() as u32)?;
        for completion in &self.matches {
            write_string(writer, completion, MAX_JSON_CHARS)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_round_trip() {
        let packet = TabComplete {
            matches: vec!["/help".to_string(), "/hat".to_string()],
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(TabComplete::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn an_empty_list_is_valid() {
        let packet = TabComplete { matches: vec![] };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [0]);
        assert_eq!(TabComplete::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn an_oversized_count_is_rejected_before_allocation() {
        // VarInt 5000 = 0x88 0x27.
        assert!(TabComplete::decode(&mut &[0x88u8, 0x27][..]).is_err());
    }
}
