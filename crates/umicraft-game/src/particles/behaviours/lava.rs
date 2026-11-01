// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lava sparks: `EntityLavaFX` (the `lava` particle of lava blocks and
//! magma-like sources).
//!
//! A spark is thrown up, falls, shrinks to nothing and sheds smoke on the way
//! ([`TickContext::spawned`]). It is always drawn at full block light
//! ([`Lighting::LavaFull`]).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::{random_life, smoke, Behaviour, TickContext};
use crate::particles::particle::{Lighting, Particle, Texture};
use crate::shapes::BlockSource;

/// The share of the random velocity kept, MCP `EntityLavaFX.java:18` (`0.8F`
/// widened); the vertical part is then replaced.
const VELOCITY_KEPT: f64 = 0.800_000_011_920_929;
/// The gravity per tick, MCP `EntityLavaFX.java:73`.
const GRAVITY: f64 = 0.03;
/// The drag per tick, MCP `EntityLavaFX.java:75` (`0.999F` widened).
const DRAG: f64 = 0.999_000_012_874_603_3;
/// The cell of `particles.png` of the spark, MCP `EntityLavaFX.java:30`.
const CELL: u8 = 49;

/// A lava spark at `position` (MCP `EntityLavaFX` constructor, lines 14-31):
/// an upward speed of `0.05..0.45`, a scale of `0.2..2.2` times the base draw
/// and a life of `16 / (r * 0.8 + 0.2)` ticks. The packet speed is ignored by
/// the factory (MCP line 105).
#[must_use]
pub(super) fn create(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion *= VELOCITY_KEPT;
    particle.motion.y = f64::from(rng.next_float() * 0.4 + 0.05);
    particle.color = [1.0; 3];
    particle.scale *= rng.next_float() * 2.0 + 0.2;
    particle.max_age = random_life(16.0, rng);
    particle.texture = Texture::Cell { index: CELL };
    particle.lighting = Lighting::LavaFull;
    particle.behaviour = Behaviour::Lava {
        base_scale: particle.scale,
    };
    particle
}

/// `EntityLavaFX.onUpdate`: age, maybe a puff of smoke, fall, move, drag. The
/// smoke is more likely early in the life: it appears when a draw exceeds the
/// fraction of the life that has passed.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.remember_position();
    particle.grow_older();
    let progress = particle.age as f32 / particle.max_age as f32;
    if context.rng.next_float() > progress {
        // `worldObj.spawnParticle(SMOKE_NORMAL, pos, motion)`: the smoke
        // factory, with the spark's own motion as its speed.
        let puff = smoke::create(
            particle.position,
            particle.motion,
            smoke::SMOKE_FACTOR,
            context.rng,
        );
        context.spawned.push(puff);
    }
    particle.motion.y -= GRAVITY;
    particle.move_with_collision(context.world);
    particle.drag_and_friction(DRAG);
}

/// The scale to draw with: `base * (1 - f^2)` (`renderParticle`, MCP
/// `EntityLavaFX.java:49-54`).
#[must_use]
pub(super) fn render_scale(base_scale: f32, particle: &Particle, partial_ticks: f32) -> f32 {
    let progress = (particle.age as f32 + partial_ticks) / particle.max_age as f32;
    base_scale * (1.0 - progress * progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_world::World;

    fn make(seed: u64) -> Particle {
        create(DVec3::new(0.5, 100.0, 0.5), &mut JavaRandom::new(seed))
    }

    fn tick(particle: &mut Particle, spawned: &mut Vec<Particle>, rng: &mut JavaRandom) {
        let world = World::new(0);
        let mut context = TickContext {
            world: &world,
            rng,
            spawned,
        };
        particle.behaviour.update(particle, &mut context);
    }

    #[test]
    fn constructor_values_stay_in_the_reference_ranges() {
        for seed in 0..200 {
            let particle = make(seed);
            assert!((0.05..=0.45).contains(&particle.motion.y));
            // base scale 1..2 times 0.2..2.2.
            assert!((0.2..=4.4).contains(&particle.scale));
            // 16 / 1.0 up to 16 / 0.2.
            assert!((16..=80).contains(&particle.max_age));
            assert_eq!(particle.texture, Texture::Cell { index: 49 });
            assert_eq!(particle.lighting, Lighting::LavaFull);
        }
    }

    #[test]
    fn spark_sheds_smoke_early_in_its_life() {
        let mut rng = JavaRandom::new(11);
        let mut spawned = Vec::new();
        let mut particle = make(3);
        particle.max_age = 1000;
        for _ in 0..50 {
            tick(&mut particle, &mut spawned, &mut rng);
        }
        // The fraction is below 0.05, so nearly every draw beats it.
        assert!(spawned.len() >= 40, "{}", spawned.len());
        assert!(spawned
            .iter()
            .all(|puff| matches!(puff.behaviour, Behaviour::Smoke { .. })));
    }

    #[test]
    fn no_smoke_once_the_life_is_over() {
        let mut rng = JavaRandom::new(11);
        let mut spawned = Vec::new();
        let mut particle = make(3);
        particle.max_age = 4;
        particle.age = 4;
        tick(&mut particle, &mut spawned, &mut rng);
        // age 5 / 4 > 1: no draw can exceed it.
        assert!(spawned.is_empty());
        assert!(particle.dead);
    }

    #[test]
    fn spark_falls_with_gravity_and_light_drag() {
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let mut particle = make(3);
        particle.motion = DVec3::ZERO;
        tick(&mut particle, &mut spawned, &mut rng);
        assert!((particle.motion.y - (-GRAVITY * DRAG)).abs() < 1e-12);
    }

    #[test]
    fn spark_shrinks_to_nothing() {
        let mut particle = make(3);
        particle.max_age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 2.0);
        particle.age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 0.0);
    }

    #[test]
    fn lava_is_lit_at_full_block_light() {
        let particle = make(3);
        assert_eq!(particle.light(0, 80, 0.5), (240, 80));
    }
}
