// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Explosion` (0x27, clientbound, play): an explosion happened; the listed
//! blocks are destroyed and the player is pushed.
//!
//! Layout (MCP `S27PacketExplosion`, `readPacketData`): centre x/y/z (f32
//! each), strength (f32), record count (i32), that many records of three
//! signed bytes (block offsets from the truncated centre), then the player
//! motion x/y/z (f32 each).
//!
//! The record count comes from the wire and the reference pre-sizes a list
//! with it, so the count is capped at what a 2 MiB frame could hold before
//! anything is allocated, and the vector only grows as records really arrive.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f32, read_i32};
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;

/// Most records a packet may declare: 3 bytes each within the 2 MiB frame limit.
const MAX_RECORDS: usize = 2 * 1024 * 1024 / 3;
/// Initial `Vec` capacity; the vector grows with the data actually received.
const INITIAL_CAPACITY: usize = 256;

/// `Explosion` (0x27, clientbound, play).
#[derive(Debug, Clone, PartialEq)]
pub struct Explosion {
    /// Centre X.
    pub x: f32,
    /// Centre Y.
    pub y: f32,
    /// Centre Z.
    pub z: f32,
    /// Explosion strength (radius).
    pub strength: f32,
    /// Destroyed blocks as `[dx, dy, dz]` offsets from the centre truncated
    /// to integers.
    pub records: Vec<[i8; 3]>,
    /// Velocity added to the player, X.
    pub player_motion_x: f32,
    /// Velocity added to the player, Y.
    pub player_motion_y: f32,
    /// Velocity added to the player, Z.
    pub player_motion_z: f32,
}

impl Explosion {
    /// Wire packet id.
    pub const ID: u8 = 0x27;

    /// The absolute positions of the destroyed blocks.
    ///
    /// The records are offsets from the centre truncated towards zero (MCP
    /// `S27PacketExplosion.readPacketData` casts the `double` centre to
    /// `int`), not floored.
    pub fn affected_blocks(&self) -> impl Iterator<Item = (i32, i32, i32)> + '_ {
        // Truncation is the reference behaviour; `as` saturates on the
        // non-finite values a hostile server could send.
        let (cx, cy, cz) = (self.x as i32, self.y as i32, self.z as i32);
        self.records.iter().map(move |[dx, dy, dz]| {
            (
                cx.saturating_add(i32::from(*dx)),
                cy.saturating_add(i32::from(*dy)),
                cz.saturating_add(i32::from(*dz)),
            )
        })
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a negative or oversized record count.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let x = read_f32(reader)?;
        let y = read_f32(reader)?;
        let z = read_f32(reader)?;
        let strength = read_f32(reader)?;
        let declared = read_i32(reader)?;
        let count = usize::try_from(declared)
            .ok()
            .filter(|&n| n <= MAX_RECORDS)
            .ok_or_else(|| {
                ProtocolError::Invariant(format!("explosion record count {declared} out of range"))
            })?;
        let mut records = Vec::with_capacity(count.min(INITIAL_CAPACITY));
        for _ in 0..count {
            records.push([read_i8(reader)?, read_i8(reader)?, read_i8(reader)?]);
        }
        Ok(Self {
            x,
            y,
            z,
            strength,
            records,
            player_motion_x: read_f32(reader)?,
            player_motion_y: read_f32(reader)?,
            player_motion_z: read_f32(reader)?,
        })
    }
}

impl EncodablePacket for Explosion {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        for value in [self.x, self.y, self.z, self.strength] {
            writer.write_all(&value.to_be_bytes())?;
        }
        // Bounded by the in-memory vector, which fits a 2 MiB frame.
        writer.write_all(&(self.records.len() as i32).to_be_bytes())?;
        for record in &self.records {
            for offset in record {
                write_i8(writer, *offset)?;
            }
        }
        for value in [
            self.player_motion_x,
            self.player_motion_y,
            self.player_motion_z,
        ] {
            writer.write_all(&value.to_be_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explosion_wire_shape() {
        let mut expected = Vec::new();
        for value in [1.0f32, 2.0, 3.0, 4.0] {
            expected.extend_from_slice(&value.to_be_bytes());
        }
        expected.extend_from_slice(&2i32.to_be_bytes());
        expected.extend([1, 0xFF, 2, 0xFE, 0, 0x7F]); // (1,-1,2) (-2,0,127)
        for value in [0.5f32, 0.0, -0.5] {
            expected.extend_from_slice(&value.to_be_bytes());
        }
        let packet = Explosion {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            strength: 4.0,
            records: vec![[1, -1, 2], [-2, 0, 127]],
            player_motion_x: 0.5,
            player_motion_y: 0.0,
            player_motion_z: -0.5,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Explosion::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_record_counts_and_truncation_are_errors() {
        let header = [0u8; 16];
        let mut negative = header.to_vec();
        negative.extend_from_slice(&(-5i32).to_be_bytes());
        assert!(Explosion::decode(&mut &negative[..]).is_err());

        let mut huge = header.to_vec();
        huge.extend_from_slice(&i32::MAX.to_be_bytes());
        assert!(Explosion::decode(&mut &huge[..]).is_err());

        // Plausible count, but the stream ends: fails without a big allocation.
        let mut short = header.to_vec();
        short.extend_from_slice(&600_000i32.to_be_bytes());
        assert!(Explosion::decode(&mut &short[..]).is_err());
    }

    #[test]
    fn affected_blocks_truncate_the_centre_towards_zero() {
        let explosion = Explosion {
            x: -0.5,
            y: 64.9,
            z: 3.2,
            strength: 4.0,
            records: vec![[1, 0, -1], [-1, -2, 0]],
            player_motion_x: 0.0,
            player_motion_y: 0.0,
            player_motion_z: 0.0,
        };
        let blocks: Vec<_> = explosion.affected_blocks().collect();
        assert_eq!(blocks, [(1, 64, 2), (-1, 62, 3)]);
    }
}
