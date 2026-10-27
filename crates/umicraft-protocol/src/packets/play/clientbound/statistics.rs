// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Statistics` (0x37, clientbound, play): the player's statistic counters.
//!
//! Layout (MCP `S37PacketStatistics`, `readPacketData`): a VarInt entry
//! count, then per entry a statistic name string (up to 32767 chars) and a
//! VarInt value. The reference client silently drops names it does not know;
//! this crate keeps every entry verbatim and leaves that choice to the
//! consumer.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

/// Longest string the reference client accepts for a name (`readStringFromBuffer(32767)`).
const MAX_NAME_CHARS: usize = 32767;

/// Largest entry count accepted from the wire. A 2 MiB packet cannot carry
/// more useful entries, and bounding it keeps a hostile count harmless.
pub(super) const MAX_LIST_LEN: usize = 65_536;

/// Reads a VarInt list length and validates it before anything is allocated.
///
/// # Errors
/// [`ProtocolError::Invariant`] when the length is negative or above
/// [`MAX_LIST_LEN`]; VarInt errors from the stream.
pub(super) fn read_list_len(reader: &mut impl Read) -> Result<usize, ProtocolError> {
    let len = read_var_i32(reader)?;
    usize::try_from(len)
        .ok()
        .filter(|&len| len <= MAX_LIST_LEN)
        .ok_or_else(|| ProtocolError::Invariant(format!("list length {len} is out of range")))
}

/// `Statistics` (0x37, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    /// `(statistic name, value)` pairs in wire order, e.g. `stat.jump`.
    pub entries: Vec<(String, i32)>,
}

impl Statistics {
    /// Wire packet id.
    pub const ID: u8 = 0x37;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a bad count or an invalid string.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let len = read_list_len(reader)?;
        let mut entries = Vec::new();
        for _ in 0..len {
            let name = read_string(reader, MAX_NAME_CHARS)?;
            entries.push((name, read_var_i32(reader)?));
        }
        Ok(Self { entries })
    }
}

impl EncodablePacket for Statistics {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entries.len() as u32)?;
        for (name, value) in &self.entries {
            write_string(writer, name, MAX_NAME_CHARS)?;
            write_var_u32(writer, *value as u32)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_wire_shape() {
        let packet = Statistics {
            entries: vec![("a".to_string(), 300)],
        };
        let expected = [1, 1, b'a', 0xAC, 0x02];
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Statistics::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_statistics_are_errors() {
        assert!(Statistics::decode(&mut &[][..]).is_err());
        assert!(Statistics::decode(&mut &[2, 1, b'a', 1][..]).is_err());
        // Negative count (-1 as a VarInt) and an absurd count.
        assert!(Statistics::decode(&mut &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F][..]).is_err());
        assert!(Statistics::decode(&mut &[0xFF, 0xFF, 0xFF, 0xFF, 0x07][..]).is_err());
    }
}
