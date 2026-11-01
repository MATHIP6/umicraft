// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block fragments: `EntityDiggingFX` and `EntityBlockDustFX`.
//!
//! A fragment is a quarter of a block texture, thrown out when a block breaks,
//! is hit, or when a `blockcrack` or `blockdust` particle is asked for. It has
//! no behaviour beyond the common `EntityFX` update.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockState;

use crate::particles::particle::{Layer, Particle, Texture};

/// The grey every fragment starts from (`EntityDiggingFX.java:25`).
const BASE_SHADE: f32 = 0.6;

/// `new EntityDiggingFX(..., state)` followed by the colour step: dark,
/// half-size and tinted by the block (MCP `EntityDiggingFX.java:21-47`).
///
/// `tint` is the colour multiplier of the block (`colorMultiplier` at the
/// block, or `getRenderColor` without a position); the grass block takes none,
/// so its caller passes white.
#[must_use]
pub fn fragment(
    state: BlockState,
    tint: [f32; 3],
    position: DVec3,
    velocity: DVec3,
    rng: &mut JavaRandom,
) -> Particle {
    let mut particle = Particle::with_velocity(position, velocity, rng);
    particle.layer = Layer::Terrain;
    particle.texture = Texture::Block { state };
    particle.gravity = 1.0;
    particle.color = tint.map(|channel| BASE_SHADE * channel);
    particle.scale /= 2.0;
    particle
}

/// `EntityBlockDustFX`: a fragment that keeps exactly the velocity it was given
/// (MCP `EntityBlockDustFX.java:11-14`), so a sprint puff drifts as asked.
#[must_use]
pub fn dust(
    state: BlockState,
    tint: [f32; 3],
    position: DVec3,
    velocity: DVec3,
    rng: &mut JavaRandom,
) -> Particle {
    let mut particle = fragment(state, tint, position, velocity, rng);
    particle.motion = velocity;
    particle
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE: BlockState = BlockState::new(1, 0);

    #[test]
    fn a_fragment_is_a_dark_half_size_block_piece() {
        let mut rng = JavaRandom::new(1);
        let piece = fragment(STONE, [1.0; 3], DVec3::ZERO, DVec3::ZERO, &mut rng);
        assert_eq!(piece.layer, Layer::Terrain);
        assert_eq!(piece.texture, Texture::Block { state: STONE });
        assert_eq!(piece.color, [0.6; 3]);
        assert_eq!(piece.gravity, 1.0);
        assert!((0.5..=1.0).contains(&piece.scale));
    }

    #[test]
    fn the_tint_multiplies_the_grey() {
        let mut rng = JavaRandom::new(1);
        let piece = fragment(STONE, [0.5, 1.0, 0.0], DVec3::ZERO, DVec3::ZERO, &mut rng);
        assert_eq!(piece.color, [0.3, 0.6, 0.0]);
    }

    #[test]
    fn dust_keeps_the_velocity_it_was_given() {
        let mut rng = JavaRandom::new(1);
        let velocity = DVec3::new(0.1, 0.2, -0.3);
        let puff = dust(STONE, [1.0; 3], DVec3::ZERO, velocity, &mut rng);
        assert_eq!(puff.motion, velocity);
    }
}
