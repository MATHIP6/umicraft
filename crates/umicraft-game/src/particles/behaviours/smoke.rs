// SPDX-License-Identifier: MIT OR Apache-2.0

//! Smoke and large smoke: `EntitySmokeFX`, and `EntityCritFX`, its subclass
//! that `EffectRenderer.registerVanillaParticles` registers for
//! `SMOKE_LARGE` (line 74).
//!
//! Both rise, spread sideways once they stop climbing, fade from the
//! `particles.png` cells 7 down to 0 and grow from nothing over the first 1/32
//! of their life. The large one is the same class with a size factor of 2.5.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::{random_life, Behaviour, TickContext};
use crate::particles::particle::{Particle, Texture};
use crate::shapes::BlockSource;

/// The share of the random velocity kept, MCP `EntitySmokeFX.java:21` (`0.1F`
/// widened).
const VELOCITY_KEPT: f64 = 0.100_000_001_490_116_12;
/// The upper bound of the grey level, MCP `EntitySmokeFX.java:27` (`0.3F`
/// widened).
const MAX_GREY: f64 = 0.300_000_011_920_929;
/// The size factor of the plain smoke.
pub(super) const SMOKE_FACTOR: f32 = 1.0;
/// The size factor of `EntityCritFX`, MCP `EntityCritFX.java:12`.
pub(super) const LARGE_SMOKE_FACTOR: f32 = 2.5;
/// The upward push per tick of the smoke, MCP `EntitySmokeFX.java:51`.
const SMOKE_LIFT: f64 = 0.004;
/// The widening of the horizontal motion while the smoke does not climb, MCP
/// `EntitySmokeFX.java:55`.
const SPREAD: f64 = 1.1;
/// The drag per tick, MCP `EntitySmokeFX.java:59` (`0.96F` widened).
const DRAG: f64 = 0.959_999_978_542_327_9;
/// The number of fading cells of the picture, MCP `EntitySmokeFX.java:50`.
const FADE_CELLS: u32 = 8;
/// How many times faster than its life the size ramps up, MCP
/// `EntitySmokeFX.java:38`.
const GROWTH_SPEED: f32 = 32.0;

/// A smoke particle (`EntitySmokeFX` constructor, MCP lines 17-35): a small
/// random drift plus `speed`, a grey level in `0..0.3`, a scale of
/// `0.75 * factor` times the base draw and a life stretched by `factor`.
#[must_use]
pub(super) fn create(position: DVec3, speed: DVec3, factor: f32, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion = particle.motion * VELOCITY_KEPT + speed;
    let grey = (rng.next_double() * MAX_GREY) as f32;
    particle.color = [grey; 3];
    particle.scale = particle.scale * 0.75 * factor;
    particle.max_age = (random_life(8.0, rng) as f32 * factor) as u32;
    particle.behaviour = Behaviour::Smoke {
        base_scale: particle.scale,
    };
    particle
}

/// `EntitySmokeFX.onUpdate`: it does not call the common update.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    drift(particle, context.world, SMOKE_LIFT);
}

/// The tick that smoke and redstone dust share (MCP `EntitySmokeFX.java:42-65`
/// and `EntityReddustFX.java:62-85`): age, pick the fading cell, push up by
/// `lift`, move, spread while not climbing, drag and friction.
pub(super) fn drift<S: BlockSource + ?Sized>(particle: &mut Particle, world: &S, lift: f64) {
    particle.remember_position();
    particle.grow_older();
    particle.texture = Texture::Cell {
        index: fading_cell(particle.age, particle.max_age),
    };
    particle.motion.y += lift;
    particle.move_with_collision(world);
    if particle.position.y == particle.previous_position.y {
        particle.motion.x *= SPREAD;
        particle.motion.z *= SPREAD;
    }
    particle.drag_and_friction(DRAG);
}

/// The cell `7 - age * 8 / max_age` of `particles.png`. The reference lets it
/// reach -1 on the last tick before death; it is clamped to 0 here because a
/// cell index is unsigned.
fn fading_cell(age: u32, max_age: u32) -> u8 {
    (FADE_CELLS - 1).saturating_sub(age * FADE_CELLS / max_age) as u8
}

/// The scale to draw with: the base scale grown over the first 1/32 of the
/// life (`renderParticle`, MCP `EntitySmokeFX.java:36-41`). Shared with the
/// redstone dust.
#[must_use]
pub(super) fn render_scale(base_scale: f32, particle: &Particle, partial_ticks: f32) -> f32 {
    let progress = (particle.age as f32 + partial_ticks) / particle.max_age as f32 * GROWTH_SPEED;
    base_scale * progress.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_world::World;

    fn make(factor: f32, seed: u64) -> Particle {
        create(
            DVec3::new(0.5, 100.0, 0.5),
            DVec3::ZERO,
            factor,
            &mut JavaRandom::new(seed),
        )
    }

    fn run(particle: &mut Particle, ticks: usize) {
        let world = World::new(0);
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        for _ in 0..ticks {
            let mut context = TickContext {
                world: &world,
                rng: &mut rng,
                spawned: &mut spawned,
            };
            particle.behaviour.update(particle, &mut context);
        }
    }

    #[test]
    fn colour_is_a_grey_below_the_bound() {
        for seed in 0..100 {
            let [r, g, b] = make(SMOKE_FACTOR, seed).color;
            assert!(r == g && g == b && (0.0..0.3).contains(&r));
        }
    }

    #[test]
    fn large_smoke_is_bigger_and_longer_lived() {
        // Same seed, same draws: only the factor differs.
        let small = make(SMOKE_FACTOR, 5);
        let large = make(LARGE_SMOKE_FACTOR, 5);
        assert!((large.scale - small.scale * 2.5).abs() < 1e-5);
        assert!(large.max_age >= small.max_age * 2);
        assert!((0.75..=1.5).contains(&small.scale));
    }

    #[test]
    fn speed_is_added_to_the_drift() {
        let speed = DVec3::new(0.0, 5.0, 0.0);
        let particle = create(DVec3::ZERO, speed, 1.0, &mut JavaRandom::new(1));
        assert!(particle.motion.y > 4.9);
    }

    #[test]
    fn smoke_rises_and_fades_its_cell() {
        let mut particle = make(SMOKE_FACTOR, 3);
        particle.max_age = 16;
        particle.motion = DVec3::ZERO;
        run(&mut particle, 4);
        // 0.004 lift, dragged each tick: it climbed.
        assert!(particle.position.y > 100.0);
        // 7 - 4 * 8 / 16.
        assert_eq!(particle.texture, Texture::Cell { index: 5 });
    }

    #[test]
    fn smoke_dies_after_its_life() {
        let mut particle = make(SMOKE_FACTOR, 3);
        particle.max_age = 5;
        run(&mut particle, 5);
        assert!(!particle.dead);
        run(&mut particle, 1);
        assert!(particle.dead);
    }

    #[test]
    fn fading_cell_never_goes_below_zero() {
        assert_eq!(fading_cell(8, 8), 0);
        assert_eq!(fading_cell(9, 8), 0);
        assert_eq!(fading_cell(1, 8), 6);
    }

    #[test]
    fn size_ramps_up_over_the_first_32nd_of_the_life() {
        let mut particle = make(SMOKE_FACTOR, 3);
        particle.max_age = 32;
        assert_eq!(render_scale(2.0, &particle, 0.5), 1.0);
        particle.age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 2.0);
    }
}
