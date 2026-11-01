// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hearts of breeding and the angry cloud of a hit villager:
//! `EntityHeartFX` and its two factories (MCP
//! `client/particle/EntityHeartFX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, cell, drag_and_rest, move_and_boost, SLOW_START};
use super::{Behaviour, TickContext};
use crate::particles::{Particle, ParticleKind};
use crate::shapes::BlockSource;

/// The cell of the heart, MCP `EntityHeartFX.java:30`.
const HEART_CELL: i32 = 80;
/// The cell of the angry villager, MCP `EntityHeartFX.java:78`.
const ANGRY_CELL: i32 = 81;
/// The scale factor of both factories, MCP `EntityHeartFX.java:13`.
const SCALE: f32 = 2.0;
/// The life, MCP `EntityHeartFX.java:28`.
const LIFE: u32 = 16;
/// Upward start speed, MCP `EntityHeartFX.java:24`.
const RISE: f64 = 0.1;
/// Per-tick drag, MCP `EntityHeartFX.java:62`.
const DRAG: f64 = 0.860_000_014_305_114_7;
/// The angry cloud floats half a block above the packet position, MCP
/// `EntityHeartFX.java:76`.
const ANGRY_LIFT: f64 = 0.5;

/// `IParticleFactory.getEntityFX` of `Heart` and `AngryVillager`; `kind` must
/// be one of the two. The packet speed is unused (the constructor passes zero
/// to `EntityFX`).
pub(super) fn create(
    kind: ParticleKind,
    position: DVec3,
    _speed: DVec3,
    rng: &mut JavaRandom,
) -> Particle {
    if kind == ParticleKind::AngryVillager {
        let mut particle = spawn(position + DVec3::new(0.0, ANGRY_LIFT, 0.0), rng);
        particle.texture = cell(ANGRY_CELL);
        particle.color = [1.0; 3];
        particle
    } else {
        spawn(position, rng)
    }
}

/// The constructor of `EntityHeartFX`: a slow rise, a fixed life of 16 ticks
/// and a scale of `1.5 * (0.5 .. 1.0) * 2`.
fn spawn(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion *= SLOW_START;
    particle.motion.y += RISE;
    particle.scale *= 0.75 * SCALE;
    particle.max_age = LIFE;
    particle.texture = cell(HEART_CELL);
    particle.behaviour = Behaviour::Heart;
    particle
}

/// `EntityHeartFX.onUpdate`: ages, moves, speeds up when level and slows down.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    begin_tick(particle);
    move_and_boost(particle, context.world);
    drag_and_rest(particle, DRAG);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Texture;
    use umicraft_world::World;

    fn make(kind: ParticleKind, seed: u64) -> Particle {
        let mut rng = JavaRandom::new(seed);
        create(kind, DVec3::new(0.5, 100.0, 0.5), DVec3::ZERO, &mut rng)
    }

    fn tick(particle: &mut Particle) {
        let world = World::new(0);
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let mut context = TickContext {
            world: &world,
            rng: &mut rng,
            spawned: &mut spawned,
        };
        let behaviour = particle.behaviour;
        behaviour.update(particle, &mut context);
    }

    #[test]
    fn constructor_values_stay_in_the_reference_ranges() {
        for seed in 0..100 {
            let heart = make(ParticleKind::Heart, seed);
            assert_eq!(heart.max_age, 16);
            // (1 .. 2) * 0.75 * 2.
            assert!((1.5..=3.0).contains(&heart.scale));
            assert_eq!(heart.texture, Texture::Cell { index: 80 });
            assert_eq!(heart.color, [1.0; 3]);
            // 0.1 plus a start motion damped to under 0.45 * 0.4 * 0.01.
            assert!((heart.motion.y - 0.1).abs() < 0.003);
        }
    }

    #[test]
    fn the_angry_villager_floats_above_and_shows_its_own_cell() {
        let angry = make(ParticleKind::AngryVillager, 4);
        assert_eq!(angry.position.y, 100.5);
        assert_eq!(angry.texture, Texture::Cell { index: 81 });
        assert_eq!(angry.color, [1.0; 3]);
        assert_eq!(angry.max_age, 16);
    }

    #[test]
    fn a_tick_rises_and_drags() {
        let mut heart = make(ParticleKind::Heart, 4);
        heart.motion = DVec3::new(0.0, 0.1, 0.0);
        tick(&mut heart);
        assert!((heart.position.y - 100.1).abs() < 1e-12);
        assert!((heart.motion.y - 0.1 * DRAG).abs() < 1e-12);
    }

    #[test]
    fn a_level_heart_speeds_up_horizontally_before_the_drag() {
        let mut heart = make(ParticleKind::Heart, 4);
        heart.motion = DVec3::new(0.01, 0.0, 0.0);
        tick(&mut heart);
        assert!((heart.motion.x - 0.01 * 1.1 * DRAG).abs() < 1e-12);
    }

    #[test]
    fn it_lives_sixteen_ticks_and_dies_on_the_next() {
        let mut heart = make(ParticleKind::Heart, 4);
        for _ in 0..16 {
            tick(&mut heart);
            assert!(!heart.dead);
        }
        tick(&mut heart);
        assert!(heart.dead);
    }
}
