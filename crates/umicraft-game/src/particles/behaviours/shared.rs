// SPDX-License-Identifier: MIT OR Apache-2.0

//! What several particle families share: the countdown life, the random
//! draws written with `Math.random()` and the tests of the block a particle
//! sits in.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::fluid::{height_percent, is_solid_material, Liquid};
use umicraft_world::{BlockPos, BlockState};

use crate::particles::particle::Particle;
use crate::shapes::BlockSource;

/// The block cell that contains `position` (`new BlockPos(entity)`).
pub(super) fn cell_of(position: DVec3) -> BlockPos {
    BlockPos::new(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

/// Whether the particle is inside a water block (`getMaterial() ==
/// Material.water`).
pub(super) fn is_in_water<S: BlockSource + ?Sized>(world: &S, position: DVec3) -> bool {
    let cell = cell_of(position);
    Liquid::Water.contains(world.block(cell.x, cell.y, cell.z))
}

/// `if (particleMaxAge-- <= 0) setDead()`: these particles count their life
/// down instead of comparing an age with it, and die once it ran out.
pub(super) fn count_down(particle: &mut Particle) {
    if particle.max_age == 0 {
        particle.dead = true;
    }
    particle.max_age = particle.max_age.saturating_sub(1);
}

/// `(int)(numerator / (Math.random() * 0.8 + 0.2))`: the life of the water
/// particles, between `numerator` and five times that.
pub(super) fn random_life(rng: &mut JavaRandom, numerator: f64) -> u32 {
    (numerator / (rng.next_double() * 0.8 + 0.2)) as u32
}

/// `(Math.random() * 2 - 1) * spread`: a random offset on both sides of 0.
pub(super) fn random_offset(rng: &mut JavaRandom, spread: f64) -> f64 {
    (rng.next_double() * 2.0 - 1.0) * spread
}

/// Whether the particle is below the top of the liquid or solid block it
/// sits in, where it dies (the tail of `EntityRainFX.onUpdate` and
/// `EntityDropParticleFX.onUpdate`).
///
/// A liquid reaches `1 - getLiquidHeightPercent(level)` of its cell; a solid
/// block reaches `solid_top`, which the two classes read differently (the
/// block's bounds for the rain, the whole cell for the drip). Air and plants
/// never kill.
pub(super) fn is_under_surface<S: BlockSource + ?Sized>(
    world: &S,
    position: DVec3,
    solid_top: impl FnOnce(BlockState, BlockPos) -> f64,
) -> bool {
    let cell = cell_of(position);
    let state = world.block(cell.x, cell.y, cell.z);
    let top = if Liquid::of(state).is_some() {
        f64::from(1.0 - height_percent(state.metadata()))
    } else if is_solid_material(state) {
        solid_top(state, cell)
    } else {
        return false;
    };
    position.y < f64::from(cell.y) + top
}

/// A world where every block below `top` is `state` and the rest is air.
#[cfg(test)]
pub(super) struct Layered {
    /// The first row (exclusive) that is not filled.
    pub top: i32,
    /// The block of the filled rows.
    pub state: BlockState,
}

#[cfg(test)]
impl BlockSource for Layered {
    fn block(&self, _x: i32, y: i32, _z: i32) -> BlockState {
        if y < self.top {
            self.state
        } else {
            BlockState::AIR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_countdown_kills_after_the_last_tick() {
        let mut particle = Particle::new(DVec3::ZERO, &mut JavaRandom::new(1));
        particle.max_age = 1;
        count_down(&mut particle);
        assert!(!particle.dead);
        count_down(&mut particle);
        assert!(particle.dead);
    }

    #[test]
    fn random_draws_stay_in_range() {
        let mut rng = JavaRandom::new(3);
        for _ in 0..500 {
            assert!((8..=40).contains(&random_life(&mut rng, 8.0)));
            assert!(random_offset(&mut rng, 0.5).abs() <= 0.5);
        }
    }

    #[test]
    fn a_source_liquid_surface_is_eight_ninths_of_the_cell() {
        let water = Layered {
            top: i32::MAX,
            state: BlockState::new(9, 0),
        };
        let solid_top = |_, _| 1.0;
        assert!(is_under_surface(
            &water,
            DVec3::new(0.5, 4.8, 0.5),
            solid_top
        ));
        assert!(!is_under_surface(
            &water,
            DVec3::new(0.5, 4.95, 0.5),
            solid_top
        ));
    }

    #[test]
    fn air_is_never_a_surface() {
        let air = Layered {
            top: 0,
            state: BlockState::AIR,
        };
        assert!(!is_under_surface(&air, DVec3::ZERO, |_, _| 1.0));
    }
}
