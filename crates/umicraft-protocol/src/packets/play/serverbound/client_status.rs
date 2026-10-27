// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Client Status` (0x16, serverbound, play): respawn and statistics requests.
//!
//! Layout (MCP `C16PacketClientStatus`, `writePacketData`): a VarInt holding
//! the ordinal of `C16PacketClientStatus.EnumState`.
//!
//! | action | ordinal |
//! |---|---|
//! | perform respawn | 0 |
//! | request stats | 1 |
//! | open inventory achievement | 2 |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_u32, write_var_u32};

/// What the client asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientStatusAction {
    /// Respawn after death.
    PerformRespawn,
    /// Send the statistics.
    RequestStats,
    /// The inventory was opened (achievement trigger).
    OpenInventoryAchievement,
}

impl ClientStatusAction {
    /// The wire ordinal of this action.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::PerformRespawn => 0,
            Self::RequestStats => 1,
            Self::OpenInventoryAchievement => 2,
        }
    }

    /// The action with the given wire ordinal, `None` for an unknown one.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::PerformRespawn),
            1 => Some(Self::RequestStats),
            2 => Some(Self::OpenInventoryAchievement),
            _ => None,
        }
    }
}

/// `Client Status` (0x16, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientStatus {
    /// The requested action.
    pub action: ClientStatusAction,
}

impl ClientStatus {
    /// Wire packet id.
    pub const ID: u8 = 0x16;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt or an unknown
    /// action ordinal.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let code = read_var_u32(reader)?;
        let action = ClientStatusAction::from_code(code)
            .ok_or_else(|| ProtocolError::Invariant(format!("unknown client status {code}")))?;
        Ok(Self { action })
    }
}

impl EncodablePacket for ClientStatus {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.action.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_status_wire_shape() {
        let packet = ClientStatus::decode(&mut &[0x01][..]).unwrap();
        assert_eq!(packet.action, ClientStatusAction::RequestStats);
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [0x01]);
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        assert!(ClientStatus::decode(&mut &[0x03][..]).is_err());
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        assert!(ClientStatus::decode(&mut &[][..]).is_err());
    }
}
