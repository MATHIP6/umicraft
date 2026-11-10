// SPDX-License-Identifier: MIT OR Apache-2.0

//! `EntityBreakingFX`: a quarter of an item's picture, thrown like a block
//! fragment. Only the slime's factory is ported (a slime or a slime-ball
//! throws these when it lands); the item-crack kind needs the item id of the
//! packet and waits for a client that knows the item sprites by id.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use crate::particles::particle::{Layer, Particle, Texture};

/// The atlas key of the slime ball (`Items.slime_ball`).
const SLIME_BALL: &str = "items/slimeball";

/// `EntityBreakingFX.SlimeFactory`: a half-size slime-ball fragment that falls
/// like snow (`Blocks.snow.blockParticleGravity`, `1.0`) and has no velocity
/// of its own beyond the base scatter (MCP `EntityBreakingFX.java:31-36`).
///
/// It is drawn on layer 1, the atlas of the blocks, which holds the items too
/// (`EntityBreakingFX.getFXLayer`).
#[must_use]
pub fn slime(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.layer = Layer::Terrain;
    particle.texture = Texture::Item { key: SLIME_BALL };
    particle.gravity = 1.0;
    particle.scale /= 2.0;
    particle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slime_fragment_is_a_half_size_white_slime_ball_piece() {
        let mut rng = JavaRandom::new(1);
        let piece = slime(DVec3::ZERO, &mut rng);
        assert_eq!(piece.layer, Layer::Terrain);
        assert_eq!(piece.texture, Texture::Item { key: SLIME_BALL });
        assert_eq!(piece.color, [1.0; 3]);
        assert_eq!(piece.gravity, 1.0);
        assert!((0.5..=1.0).contains(&piece.scale));
    }
}
