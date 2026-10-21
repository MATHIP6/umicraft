// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Action` (0x0B, serverbound, play): the player changed one of an
//! entity's states — sneaking, sprinting, sleeping.
//!
//! Layout (MCP `C0BPacketEntityAction`, `readPacketData`): VarInt entity id,
//! an action byte (the ordinal of `C0BPacketEntityAction.Action`) and a
//! VarInt with auxiliary data.
//!
//! | action | ordinal | aux data |
//! |---|---|---|
//! | start sneaking | 0 | — |
//! | stop sneaking | 1 | — |
//! | stop sleeping | 2 | bed position, if the player slept in one |
//! | start sprinting | 3 | — |
//! | stop sprinting | 4 | — |
//! | riding jump | 5 | — |
//! | open inventory | 6 | — |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// The state change an entity action describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityActionKind {
    /// The entity started sneaking.
    StartSneaking,
    /// The entity stopped sneaking.
    StopSneaking,
    /// The entity stopped sleeping.
    StopSleeping,
    /// The entity started sprinting.
    StartSprinting,
    /// The entity stopped sprinting.
    StopSprinting,
    /// The entity jumped while riding.
    RidingJump,
    /// The entity opened its inventory.
    OpenInventory,
}

impl EntityActionKind {
    /// The wire ordinal of this action.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::StartSneaking => 0,
            Self::StopSneaking => 1,
            Self::StopSleeping => 2,
            Self::StartSprinting => 3,
            Self::StopSprinting => 4,
            Self::RidingJump => 5,
            Self::OpenInventory => 6,
        }
    }

    /// The action with the given wire ordinal, `None` for an unknown one.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::StartSneaking),
            1 => Some(Self::StopSneaking),
            2 => Some(Self::StopSleeping),
            3 => Some(Self::StartSprinting),
            4 => Some(Self::StopSprinting),
            5 => Some(Self::RidingJump),
            6 => Some(Self::OpenInventory),
            _ => None,
        }
    }
}

/// `Entity Action` (0x0B, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityAction {
    /// The entity the action belongs to.
    pub entity_id: i32,
    /// The state change.
    pub action: EntityActionKind,
    /// Auxiliary data (the bed position when the player stops sleeping).
    pub aux_data: i32,
}

impl EntityAction {
    /// Wire packet id.
    pub const ID: u8 = 0x0B;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, or an unknown
    /// action ordinal.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let code = read_u8(reader)?;
        let action = EntityActionKind::from_code(code)
            .ok_or_else(|| ProtocolError::Invariant(format!("unknown entity action {code}")))?;
        Ok(Self {
            entity_id,
            action,
            aux_data: read_var_i32(reader)?,
        })
    }
}

impl EncodablePacket for EntityAction {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        writer.write_all(&[self.action.code()])?;
        write_var_u32(writer, self.aux_data as u32)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_action_wire_shape() {
        let bytes = [
            0xAE, 0xBD, 0x01, // entity id 24238
            0x03, // start sprinting
            0x00, // no aux data
        ];
        let packet = EntityAction::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.entity_id, 24_238);
        assert_eq!(packet.action, EntityActionKind::StartSprinting);
        assert_eq!(packet.aux_data, 0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn the_aux_data_is_a_varint() {
        let bytes = [0x01, 0x02, 0x80, 0x01];
        let packet = EntityAction::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.action, EntityActionKind::StopSleeping);
        assert_eq!(packet.aux_data, 128);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        let bytes = [0x01, 0x09, 0x00];
        assert!(EntityAction::decode(&mut &bytes[..]).is_err());
    }
}
