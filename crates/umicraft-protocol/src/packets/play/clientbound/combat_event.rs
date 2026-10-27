// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Combat Event` (0x42, clientbound, play): combat tracking events.
//!
//! Layout (MCP `S42PacketCombatEvent`, `readPacketData`): a VarInt event
//! (0 enter combat, 1 end combat, 2 entity died). End combat adds a VarInt
//! duration and an i32 entity id; entity died adds a VarInt player id, an
//! i32 entity id and the death message string (up to 32767 chars). The
//! reference client indexes an enum with the event and crashes on an unknown
//! one; here it is [`ProtocolError::Invariant`].

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i32;
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

const MAX_MESSAGE_CHARS: usize = 32767;

/// `Combat Event` (0x42, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombatEvent {
    /// The player entered combat.
    EnterCombat,
    /// Combat ended.
    EndCombat {
        /// Combat duration in ticks.
        duration: i32,
        /// Entity id of the opponent.
        entity_id: i32,
    },
    /// A tracked entity (usually the player) died.
    EntityDied {
        /// Entity id of the dead player.
        player_id: i32,
        /// Entity id of the killer.
        entity_id: i32,
        /// Death message chat JSON.
        message: String,
    },
}

impl CombatEvent {
    /// Wire packet id.
    pub const ID: u8 = 0x42;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, an invalid string or an unknown event.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        match read_var_i32(reader)? {
            0 => Ok(Self::EnterCombat),
            1 => Ok(Self::EndCombat {
                duration: read_var_i32(reader)?,
                entity_id: read_i32(reader)?,
            }),
            2 => Ok(Self::EntityDied {
                player_id: read_var_i32(reader)?,
                entity_id: read_i32(reader)?,
                message: read_string(reader, MAX_MESSAGE_CHARS)?,
            }),
            other => Err(ProtocolError::Invariant(format!(
                "unknown combat event {other}"
            ))),
        }
    }
}

impl EncodablePacket for CombatEvent {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match self {
            Self::EnterCombat => write_var_u32(writer, 0),
            Self::EndCombat {
                duration,
                entity_id,
            } => {
                write_var_u32(writer, 1)?;
                write_var_u32(writer, *duration as u32)?;
                writer.write_all(&entity_id.to_be_bytes())?;
                Ok(())
            }
            Self::EntityDied {
                player_id,
                entity_id,
                message,
            } => {
                write_var_u32(writer, 2)?;
                write_var_u32(writer, *player_id as u32)?;
                writer.write_all(&entity_id.to_be_bytes())?;
                write_string(writer, message, MAX_MESSAGE_CHARS)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(packet: CombatEvent, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(CombatEvent::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn combat_event_wire_shapes() {
        check(CombatEvent::EnterCombat, &[0]);
        check(
            CombatEvent::EndCombat {
                duration: 40,
                entity_id: 7,
            },
            &[1, 40, 0, 0, 0, 7],
        );
        check(
            CombatEvent::EntityDied {
                player_id: 3,
                entity_id: -1,
                message: "{}".to_string(),
            },
            &[2, 3, 0xFF, 0xFF, 0xFF, 0xFF, 2, b'{', b'}'],
        );
    }

    #[test]
    fn hostile_combat_event_is_an_error() {
        assert!(CombatEvent::decode(&mut &[][..]).is_err());
        assert!(CombatEvent::decode(&mut &[3][..]).is_err());
        assert!(CombatEvent::decode(&mut &[1, 40, 0, 0][..]).is_err());
        assert!(CombatEvent::decode(&mut &[2, 3, 0, 0, 0, 1, 5, b'a'][..]).is_err());
    }
}
