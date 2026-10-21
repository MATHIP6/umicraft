// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Player Digging` (0x07, serverbound, play): the player starts, stops or
//! aborts breaking a block.
//!
//! Layout (MCP `C07PacketPlayerDigging`, `readPacketData`): a status byte
//! (the ordinal of `C07PacketPlayerDigging.Action`), one packed 1.8 block
//! position (i64) and a face byte (the index of `EnumFacing`).
//!
//! | status | ordinal |
//! |---|---|
//! | start destroying the block | 0 |
//! | abort destroying the block | 1 |
//! | stop destroying the block | 2 |
//! | drop all items | 3 |
//! | drop one item | 4 |
//! | release the use item | 5 |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::play::facing::Facing;
use crate::packets::play::position::{pack_block_position, unpack_block_position};
use crate::packets::EncodablePacket;

/// The stage of a block-breaking interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiggingStatus {
    /// The player started breaking the block.
    StartDestroyBlock,
    /// The player aborted breaking the block.
    AbortDestroyBlock,
    /// The player finished breaking the block.
    StopDestroyBlock,
    /// The player dropped everything.
    DropAllItems,
    /// The player dropped the block.
    DropItem,
    /// The player released the "use item" button.
    ReleaseUseItem,
}

impl DiggingStatus {
    /// The wire ordinal of this status.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::StartDestroyBlock => 0,
            Self::AbortDestroyBlock => 1,
            Self::StopDestroyBlock => 2,
            Self::DropAllItems => 3,
            Self::DropItem => 4,
            Self::ReleaseUseItem => 5,
        }
    }

    /// The status with the given wire ordinal, `None` for an unknown one.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::StartDestroyBlock),
            1 => Some(Self::AbortDestroyBlock),
            2 => Some(Self::StopDestroyBlock),
            3 => Some(Self::DropAllItems),
            4 => Some(Self::DropItem),
            5 => Some(Self::ReleaseUseItem),
            _ => None,
        }
    }
}

/// `Player Digging` (0x07, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerDigging {
    /// The stage of the interaction.
    pub status: DiggingStatus,
    /// Block X of the targeted block.
    pub x: i32,
    /// Block Y of the targeted block.
    pub y: i32,
    /// Block Z of the targeted block.
    pub z: i32,
    /// The face of the block that was hit.
    pub face: Facing,
}

impl PlayerDigging {
    /// Wire packet id.
    pub const ID: u8 = 0x07;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an unknown status/face byte.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let status = read_u8(reader)?;
        let status = DiggingStatus::from_code(status).ok_or_else(|| {
            ProtocolError::Invariant(format!("unknown player digging status {status}"))
        })?;
        let (x, y, z) = unpack_block_position(crate::packets::codec::read_i64(reader)?);
        let face = read_u8(reader)?;
        let face = Facing::from_code(face)
            .ok_or_else(|| ProtocolError::Invariant(format!("unknown block face {face}")))?;
        Ok(Self {
            status,
            x,
            y,
            z,
            face,
        })
    }
}

impl EncodablePacket for PlayerDigging {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.status.code()])?;
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.face.code()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_digging_wire_shape() {
        let mut expected = Vec::new();
        expected.push(0); // start destroying the block
        expected.extend_from_slice(&pack_block_position(10, 64, -3).to_be_bytes());
        expected.push(1); // face: up

        let packet = PlayerDigging {
            status: DiggingStatus::StartDestroyBlock,
            x: 10,
            y: 64,
            z: -3,
            face: Facing::Up,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);

        let decoded = PlayerDigging::decode(&mut &expected[..]).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn an_unknown_status_or_face_is_rejected() {
        let mut bytes = vec![9u8];
        bytes.extend_from_slice(&pack_block_position(0, 0, 0).to_be_bytes());
        bytes.push(0);
        assert!(PlayerDigging::decode(&mut &bytes[..]).is_err());

        bytes[0] = 0;
        let last = bytes.len() - 1;
        bytes[last] = 9;
        assert!(PlayerDigging::decode(&mut &bytes[..]).is_err());
    }
}
