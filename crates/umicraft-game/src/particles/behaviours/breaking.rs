// SPDX-License-Identifier: MIT OR Apache-2.0

//! `EntityBreakingFX`: a quarter of an item's picture, thrown like a block
//! fragment. It makes the item cracks (`iconcrack`), the snowball poof and
//! the pieces of slime a landing slime throws.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use crate::particles::particle::{Layer, Particle, Texture};

/// The ids of the items the fixed factories show (`Items.slime_ball`,
/// `Items.snowball`).
const SLIME_BALL: u16 = 341;
const SNOWBALL: u16 = 332;

/// The velocity that is kept of the base scatter when the packet gives one
/// (`EntityBreakingFX.java:17-19`).
const SCATTER_KEPT: f64 = 0.100_000_001_490_116_12;

/// `new EntityBreakingFX(..., item, damage)`: white, half-size and falling like
/// snow (`Blocks.snow.blockParticleGravity`, `1.0`), on layer 1, the atlas of
/// the blocks, which holds the items too (MCP `EntityBreakingFX.java:31-36`).
fn piece(position: DVec3, id: u16, damage: u16, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.layer = Layer::Terrain;
    particle.texture = Texture::Item { id, damage };
    particle.gravity = 1.0;
    particle.scale /= 2.0;
    particle
}

/// `EntityBreakingFX.Factory` (`iconcrack`): a piece of item `id` with
/// `damage`, thrown with the packet's velocity on top of a tenth of the base
/// scatter (`EntityBreakingFX.java:17-26`).
#[must_use]
pub fn item_crack(
    position: DVec3,
    speed: DVec3,
    id: u16,
    damage: u16,
    rng: &mut JavaRandom,
) -> Particle {
    let mut particle = piece(position, id, damage, rng);
    particle.motion = particle.motion * SCATTER_KEPT + speed;
    particle
}

/// `EntityBreakingFX.SnowballFactory`: a piece of snowball.
#[must_use]
pub fn snowball(position: DVec3, rng: &mut JavaRandom) -> Particle {
    piece(position, SNOWBALL, 0, rng)
}

/// `EntityBreakingFX.SlimeFactory`: a piece of slime ball.
#[must_use]
pub fn slime(position: DVec3, rng: &mut JavaRandom) -> Particle {
    piece(position, SLIME_BALL, 0, rng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slime_fragment_is_a_half_size_white_slime_ball_piece() {
        let mut rng = JavaRandom::new(1);
        let piece = slime(DVec3::ZERO, &mut rng);
        assert_eq!(piece.layer, Layer::Terrain);
        assert_eq!(
            piece.texture,
            Texture::Item {
                id: SLIME_BALL,
                damage: 0
            }
        );
        assert_eq!(piece.color, [1.0; 3]);
        assert_eq!(piece.gravity, 1.0);
        assert!((0.5..=1.0).contains(&piece.scale));
    }

    #[test]
    fn a_snowball_poof_is_a_piece_of_snowball() {
        let mut rng = JavaRandom::new(1);
        let poof = snowball(DVec3::ZERO, &mut rng);
        assert_eq!(
            poof.texture,
            Texture::Item {
                id: SNOWBALL,
                damage: 0
            }
        );
    }

    #[test]
    fn an_item_crack_flies_with_the_packet_velocity() {
        let mut rng = JavaRandom::new(1);
        let speed = DVec3::new(0.5, 0.25, -0.5);
        let crack = item_crack(DVec3::ZERO, speed, 297, 0, &mut rng);
        assert_eq!(crack.texture, Texture::Item { id: 297, damage: 0 });
        // The base scatter keeps a tenth: the velocity stays close to the packet's.
        assert!((crack.motion - speed).length() < 0.2, "{}", crack.motion);
        let still = item_crack(DVec3::ZERO, DVec3::ZERO, 297, 0, &mut rng);
        assert!(still.motion.length() < 0.2);
    }
}
