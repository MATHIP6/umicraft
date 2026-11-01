// SPDX-License-Identifier: MIT OR Apache-2.0

//! The snow thrown by a shovel or a snowball: `EntitySnowShovelFX` (MCP
//! `client/particle/EntitySnowShovelFX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, drag_and_rest, light_grey, step_cell};
use super::{Behaviour, TickContext};
use crate::particles::Particle;
use crate::shapes::BlockSource;

/// Damping of the random start motion, MCP `EntitySnowShovelFX.java:21-23`.
const START_DAMPING: f64 = 0.100_000_001_490_116_12;
/// Downward pull per tick, MCP `EntitySnowShovelFX.java:58`.
const GRAVITY: f64 = 0.03;
/// Per-tick drag, MCP `EntitySnowShovelFX.java:60-62`.
const DRAG: f64 = 0.990_000_009_536_743_2;

/// `IParticleFactory.getEntityFX` of `SnowShovel` (scale factor 1): the packet
/// speed is added to a weak random motion.
pub(super) fn create(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    particle.motion = particle.motion * START_DAMPING + speed;
    particle.color = light_grey(rng);
    particle.scale *= 0.75;
    particle.max_age = (8.0 / (rng.next_double() * 0.8 + 0.2)) as u32;
    particle.behaviour = Behaviour::SnowShovel;
    particle
}

/// `EntitySnowShovelFX.onUpdate`: ages, steps through eight pictures, falls
/// and moves with a light drag.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    begin_tick(particle);
    particle.texture = step_cell(0, particle);
    particle.motion.y -= GRAVITY;
    particle.move_with_collision(context.world);
    drag_and_rest(particle, DRAG);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Texture;
    use umicraft_world::World;

    fn make(seed: u64, speed: DVec3) -> Particle {
        let mut rng = JavaRandom::new(seed);
        create(DVec3::new(0.5, 100.0, 0.5), speed, &mut rng)
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
            let snow = make(seed, DVec3::ZERO);
            let grey = snow.color[0];
            assert!((0.7..=1.0).contains(&grey));
            assert_eq!(snow.color, [grey; 3]);
            // (1 .. 2) * 0.75.
            assert!((0.75..=1.5).contains(&snow.scale));
            // 8 / (0.2 .. 1.0) is 8 to 40.
            assert!((8..=40).contains(&snow.max_age));
        }
    }

    #[test]
    fn the_packet_speed_is_added_to_the_weakened_throw() {
        let mut rng = JavaRandom::new(3);
        let speed = DVec3::new(0.5, 0.25, 0.0);
        let snow = create(DVec3::ZERO, speed, &mut rng);
        let mut rng = JavaRandom::new(3);
        let thrown = Particle::with_velocity(DVec3::ZERO, speed, &mut rng);
        let expected = thrown.motion * START_DAMPING + speed;
        assert!((snow.motion - expected).length() < 1e-12);
    }

    #[test]
    fn a_tick_pulls_down_before_the_move_and_drags() {
        let mut snow = make(3, DVec3::ZERO);
        snow.max_age = 40;
        snow.motion = DVec3::ZERO;
        tick(&mut snow);
        assert!((snow.position.y - (100.0 - GRAVITY)).abs() < 1e-12);
        assert!((snow.motion.y + GRAVITY * DRAG).abs() < 1e-12);
        assert_eq!(snow.texture, Texture::Cell { index: 7 });
    }

    #[test]
    fn it_dies_after_its_life() {
        let mut snow = make(3, DVec3::ZERO);
        snow.max_age = 10;
        for _ in 0..10 {
            tick(&mut snow);
            assert!(!snow.dead);
        }
        tick(&mut snow);
        assert!(snow.dead);
    }
}
