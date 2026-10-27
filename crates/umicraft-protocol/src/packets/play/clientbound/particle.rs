// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Particle` (0x2A, clientbound, play): spawn particles.
//!
//! Layout (MCP `S2APacketParticles`, `readPacketData`): particle id (i32),
//! long-distance flag (bool), position x/y/z (f32), offset x/y/z (f32),
//! speed (f32), count (i32), then as many VarInt arguments as the particle
//! type declares (`EnumParticleTypes.getArgumentCount`): `iconcrack` (36) has
//! two (item id, damage), `blockcrack` (37) and `blockdust` (38) one (block
//! state id), every other type none.
//!
//! The reference maps an unknown particle id to `BARRIER` (35, no arguments)
//! instead of failing; this decoder does the same so a newer server cannot
//! desynchronise the stream, and records that in [`Particle::particle_id`].

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_f32, read_i32};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// Id the reference substitutes for unknown particle ids (`BARRIER`).
const BARRIER_ID: i32 = 35;
/// Highest particle id known to 1.8 (`MOB_APPEARANCE`).
const MAX_PARTICLE_ID: i32 = 41;

/// Number of trailing VarInt arguments of a particle type.
fn argument_count(particle_id: i32) -> usize {
    match particle_id {
        36 => 2,
        37 | 38 => 1,
        _ => 0,
    }
}

/// `Particle` (0x2A, clientbound, play).
#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    /// Particle type id (0 to 41). Unknown wire ids are decoded as 35 (barrier).
    pub particle_id: i32,
    /// True to render from further away than the default 16 blocks.
    pub long_distance: bool,
    /// Centre X.
    pub x: f32,
    /// Centre Y.
    pub y: f32,
    /// Centre Z.
    pub z: f32,
    /// Random spread X.
    pub offset_x: f32,
    /// Random spread Y.
    pub offset_y: f32,
    /// Random spread Z.
    pub offset_z: f32,
    /// Particle speed / extra data.
    pub speed: f32,
    /// Number of particles to spawn.
    pub count: i32,
    /// Type-specific arguments; its length is fixed by `particle_id`.
    pub arguments: Vec<i32>,
}

impl Particle {
    /// Wire packet id.
    pub const ID: u8 = 0x2A;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let wire_id = read_i32(reader)?;
        let particle_id = if (0..=MAX_PARTICLE_ID).contains(&wire_id) {
            wire_id
        } else {
            BARRIER_ID
        };
        let long_distance = read_bool(reader)?;
        let x = read_f32(reader)?;
        let y = read_f32(reader)?;
        let z = read_f32(reader)?;
        let offset_x = read_f32(reader)?;
        let offset_y = read_f32(reader)?;
        let offset_z = read_f32(reader)?;
        let speed = read_f32(reader)?;
        let count = read_i32(reader)?;
        let arguments = (0..argument_count(particle_id))
            .map(|_| read_var_i32(reader))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            particle_id,
            long_distance,
            x,
            y,
            z,
            offset_x,
            offset_y,
            offset_z,
            speed,
            count,
            arguments,
        })
    }
}

impl EncodablePacket for Particle {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        if self.arguments.len() != argument_count(self.particle_id) {
            return Err(ProtocolError::Invariant(format!(
                "particle {} takes {} arguments, got {}",
                self.particle_id,
                argument_count(self.particle_id),
                self.arguments.len()
            )));
        }
        writer.write_all(&self.particle_id.to_be_bytes())?;
        writer.write_all(&[u8::from(self.long_distance)])?;
        for value in [
            self.x,
            self.y,
            self.z,
            self.offset_x,
            self.offset_y,
            self.offset_z,
            self.speed,
        ] {
            writer.write_all(&value.to_be_bytes())?;
        }
        writer.write_all(&self.count.to_be_bytes())?;
        for argument in &self.arguments {
            write_var_u32(writer, *argument as u32)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(particle_id: i32, arguments: Vec<i32>) -> Particle {
        Particle {
            particle_id,
            long_distance: true,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            offset_x: 0.1,
            offset_y: 0.2,
            offset_z: 0.3,
            speed: 0.5,
            count: 4,
            arguments,
        }
    }

    #[test]
    fn particle_wire_shape_with_block_argument() {
        let packet = sample(37, vec![4097]);
        let mut expected = Vec::new();
        expected.extend_from_slice(&37i32.to_be_bytes());
        expected.push(1);
        for value in [1.0f32, 2.0, 3.0, 0.1, 0.2, 0.3, 0.5] {
            expected.extend_from_slice(&value.to_be_bytes());
        }
        expected.extend_from_slice(&4i32.to_be_bytes());
        expected.extend([0x81, 0x20]); // VarInt 4097

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Particle::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn argument_counts_follow_the_particle_type() {
        for (id, args) in [(0, vec![]), (36, vec![1, 2]), (38, vec![7])] {
            let packet = sample(id, args);
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(Particle::decode(&mut &buffer[..]).unwrap(), packet);
        }
        let mut buffer = Vec::new();
        assert!(sample(36, vec![1]).encode_body(&mut buffer).is_err());
    }

    #[test]
    fn an_unknown_particle_id_decodes_as_barrier_like_the_reference() {
        let mut buffer = Vec::new();
        sample(0, vec![]).encode_body(&mut buffer).unwrap();
        buffer[..4].copy_from_slice(&999i32.to_be_bytes());
        let decoded = Particle::decode(&mut &buffer[..]).unwrap();
        assert_eq!(decoded.particle_id, 35);
        assert!(decoded.arguments.is_empty());
    }

    #[test]
    fn a_truncated_particle_is_an_error() {
        assert!(Particle::decode(&mut &[][..]).is_err());
        let mut buffer = Vec::new();
        sample(36, vec![1, 2]).encode_body(&mut buffer).unwrap();
        assert!(Particle::decode(&mut &buffer[..buffer.len() - 1][..]).is_err());
    }
}
