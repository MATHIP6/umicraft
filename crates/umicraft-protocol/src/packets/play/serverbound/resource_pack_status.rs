// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Resource Pack Status` (0x19, serverbound, play): the client's answer to a
//! server resource pack offer.
//!
//! Layout (MCP `C19PacketResourcePackStatus`, `writePacketData`): the pack
//! hash String (at most 40 characters), then a VarInt holding the ordinal of
//! `C19PacketResourcePackStatus.Action`.
//!
//! | result | ordinal |
//! |---|---|
//! | successfully loaded | 0 |
//! | declined | 1 |
//! | failed download | 2 |
//! | accepted | 3 |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_u32, write_var_u32};

/// Character cap of the pack hash, as in the reference.
const MAX_HASH_CHARS: usize = 40;

/// The outcome the client reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourcePackResult {
    /// The pack was downloaded and applied.
    SuccessfullyLoaded,
    /// The player declined the pack.
    Declined,
    /// The download failed.
    FailedDownload,
    /// The player accepted the offer; the download starts.
    Accepted,
}

impl ResourcePackResult {
    /// The wire ordinal of this result.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::SuccessfullyLoaded => 0,
            Self::Declined => 1,
            Self::FailedDownload => 2,
            Self::Accepted => 3,
        }
    }

    /// The result with the given wire ordinal, `None` for an unknown one.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::SuccessfullyLoaded),
            1 => Some(Self::Declined),
            2 => Some(Self::FailedDownload),
            3 => Some(Self::Accepted),
            _ => None,
        }
    }
}

/// `Resource Pack Status` (0x19, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePackStatus {
    /// Hash of the pack the answer refers to.
    pub hash: String,
    /// What happened to the pack.
    pub result: ResourcePackResult,
}

impl ResourcePackStatus {
    /// Wire packet id.
    pub const ID: u8 = 0x19;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an over-long hash, invalid UTF-8 or an
    /// unknown result ordinal.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let hash = read_string(reader, MAX_HASH_CHARS)?;
        let code = read_var_u32(reader)?;
        let result = ResourcePackResult::from_code(code).ok_or_else(|| {
            ProtocolError::Invariant(format!("unknown resource pack result {code}"))
        })?;
        Ok(Self { hash, result })
    }
}

impl EncodablePacket for ResourcePackStatus {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.hash, MAX_HASH_CHARS)?;
        write_var_u32(writer, self.result.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_pack_status_wire_shape() {
        let bytes = [2, b'a', b'b', 0x03];
        let packet = ResourcePackStatus::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.hash, "ab");
        assert_eq!(packet.result, ResourcePackResult::Accepted);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn an_unknown_result_is_rejected() {
        assert!(ResourcePackStatus::decode(&mut &[0, 0x04][..]).is_err());
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        assert!(ResourcePackStatus::decode(&mut &[2, b'a'][..]).is_err());
    }
}
