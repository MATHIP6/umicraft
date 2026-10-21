// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Use Entity` (0x02, serverbound, play): the player interacts with an
//! entity.
//!
//! Layout (MCP `C02PacketUseEntity`, `readPacketData`): VarInt target entity
//! id, an action byte (the ordinal of `C02PacketUseEntity.Action`), and —
//! only for `INTERACT_AT` — three `f32` values with the exact point the
//! player clicked.
//!
//! | action | ordinal | extra fields |
//! |---|---|---|
//! | interact | 0 | — |
//! | attack | 1 | — |
//! | interact at | 2 | hit X, Y, Z (`f32` each) |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_f32;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// What the player did with the target entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UseEntityAction {
    /// Right-click the entity (use an item on it, open a chest, …).
    Interact,
    /// Left-click the entity (attack it).
    Attack,
    /// Right-click a specific point of the entity.
    InteractAt {
        /// Hit X in world coordinates.
        x: f32,
        /// Hit Y in world coordinates.
        y: f32,
        /// Hit Z in world coordinates.
        z: f32,
    },
}

impl UseEntityAction {
    /// The wire ordinal of this action.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Interact => 0,
            Self::Attack => 1,
            Self::InteractAt { .. } => 2,
        }
    }
}

/// `Use Entity` (0x02, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UseEntity {
    /// The entity the player interacted with.
    pub target: i32,
    /// What the player did.
    pub action: UseEntityAction,
}

impl UseEntity {
    /// Wire packet id.
    pub const ID: u8 = 0x02;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, or an unknown
    /// action ordinal.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let target = read_var_i32(reader)?;
        let mut code = [0u8; 1];
        reader.read_exact(&mut code)?;
        let action = match code[0] {
            0 => UseEntityAction::Interact,
            1 => UseEntityAction::Attack,
            2 => UseEntityAction::InteractAt {
                x: read_f32(reader)?,
                y: read_f32(reader)?,
                z: read_f32(reader)?,
            },
            other => {
                return Err(ProtocolError::Invariant(format!(
                    "unknown use-entity action {other}"
                )))
            }
        };
        Ok(Self { target, action })
    }
}

impl EncodablePacket for UseEntity {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.target as u32)?;
        writer.write_all(&[self.action.code()])?;
        if let UseEntityAction::InteractAt { x, y, z } = self.action {
            writer.write_all(&x.to_be_bytes())?;
            writer.write_all(&y.to_be_bytes())?;
            writer.write_all(&z.to_be_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attack_carries_only_the_target_and_the_action() {
        let bytes = [0xAE, 0xBD, 0x01, 0x01];
        let packet = UseEntity::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.target, 24_238);
        assert_eq!(packet.action, UseEntityAction::Attack);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn interact_at_carries_the_hit_point() {
        let mut expected = Vec::new();
        expected.push(0x01);
        expected.push(0x02);
        expected.extend_from_slice(&1.5f32.to_be_bytes());
        expected.extend_from_slice(&64.0f32.to_be_bytes());
        expected.extend_from_slice(&(-2.25f32).to_be_bytes());

        let packet = UseEntity {
            target: 1,
            action: UseEntityAction::InteractAt {
                x: 1.5,
                y: 64.0,
                z: -2.25,
            },
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);

        let decoded = UseEntity::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        let bytes = [0x01, 0x07];
        assert!(UseEntity::decode(&mut &bytes[..]).is_err());
    }
}
