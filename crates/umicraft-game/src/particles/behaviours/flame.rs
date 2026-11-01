// SPDX-License-Identifier: MIT OR Apache-2.0

//! Flame: `EntityFlameFX` (torches, furnaces, spawners).
//!
//! A flame ignores the blocks (`noClip`), shrinks as it ages and glows more
//! and more ([`Lighting::FlameGlow`]). Its `onUpdate` does not call the common
//! one: no gravity, a drag of 0.96.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::{random_life, Behaviour, TickContext};
use crate::particles::particle::{Lighting, Particle, Texture};
use crate::shapes::BlockSource;

/// The share of the random velocity kept, MCP `EntityFlameFX.java:17` (`0.01`
/// widened).
const VELOCITY_KEPT: f64 = 0.009_999_999_776_482_582;
/// The spread of the starting position per axis, MCP `EntityFlameFX.java:20`.
const POSITION_JITTER: f32 = 0.05;
/// The drag per tick, MCP `EntityFlameFX.java:76` (`0.96F` widened).
const DRAG: f64 = 0.959_999_978_542_327_9;
/// The cell of `particles.png` of the flame, MCP `EntityFlameFX.java:28`.
const CELL: u8 = 48;

/// A flame at `position` moving with `speed` (MCP `EntityFlameFX`
/// constructor, lines 14-29): the random drift is cut to 1% and `speed` is
/// added, the start is jittered by up to 0.05, the colour is white and the
/// life is `8 / (r * 0.8 + 0.2) + 4` ticks.
#[must_use]
pub(super) fn create(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    particle.motion = particle.motion * VELOCITY_KEPT + speed;
    let mut jitter = || f64::from((rng.next_float() - rng.next_float()) * POSITION_JITTER);
    particle.position += DVec3::new(jitter(), jitter(), jitter());
    particle.color = [1.0; 3];
    particle.max_age = random_life(8.0, rng) + 4;
    particle.no_clip = true;
    particle.texture = Texture::Cell { index: CELL };
    particle.lighting = Lighting::FlameGlow;
    particle.behaviour = Behaviour::Flame {
        base_scale: particle.scale,
    };
    particle
}

/// `EntityFlameFX.onUpdate`.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.remember_position();
    particle.grow_older();
    particle.move_with_collision(context.world);
    particle.drag_and_friction(DRAG);
}

/// The scale to draw with: `base * (1 - f^2 / 2)` where `f` is the fraction
/// of the life (`renderParticle`, MCP `EntityFlameFX.java:34-39`).
#[must_use]
pub(super) fn render_scale(base_scale: f32, particle: &Particle, partial_ticks: f32) -> f32 {
    let progress = (particle.age as f32 + partial_ticks) / particle.max_age as f32;
    base_scale * (1.0 - progress * progress * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_world::World;

    fn make(speed: DVec3, seed: u64) -> Particle {
        create(
            DVec3::new(0.5, 100.0, 0.5),
            speed,
            &mut JavaRandom::new(seed),
        )
    }

    fn tick(particle: &mut Particle) {
        let world = World::new(0);
        let mut rng = JavaRandom::new(2);
        let mut spawned = Vec::new();
        let mut context = TickContext {
            world: &world,
            rng: &mut rng,
            spawned: &mut spawned,
        };
        particle.behaviour.update(particle, &mut context);
    }

    #[test]
    fn constructor_values_stay_in_the_reference_ranges() {
        for seed in 0..100 {
            let particle = make(DVec3::ZERO, seed);
            // 8 / 1.0 + 4 up to 8 / 0.2 + 4.
            assert!(
                (12..=44).contains(&particle.max_age),
                "{}",
                particle.max_age
            );
            assert_eq!(particle.color, [1.0; 3]);
            assert!(particle.no_clip);
            assert_eq!(particle.texture, Texture::Cell { index: 48 });
            assert_eq!(particle.lighting, Lighting::FlameGlow);
            assert!(
                (particle.position - DVec3::new(0.5, 100.0, 0.5))
                    .abs()
                    .max_element()
                    < 0.05
            );
        }
    }

    #[test]
    fn motion_is_the_speed_plus_one_percent_of_the_draw() {
        let speed = DVec3::new(0.0, 0.02, 0.0);
        for seed in 0..50 {
            let motion = make(speed, seed).motion;
            assert!((motion - speed).abs().max_element() < 0.005);
        }
    }

    #[test]
    fn flame_does_not_fall_and_slows_by_the_drag() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.motion = DVec3::new(1.0, 0.0, 0.0);
        particle.position = DVec3::new(0.5, 100.0, 0.5);
        tick(&mut particle);
        assert_eq!(particle.position.x, 1.5);
        assert_eq!(particle.position.y, 100.0);
        assert!((particle.motion.x - DRAG).abs() < 1e-12);
    }

    #[test]
    fn flame_passes_through_blocks() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.position = DVec3::new(0.5, -5.0, 0.5);
        particle.motion = DVec3::ZERO;
        tick(&mut particle);
        assert_eq!(particle.position.y, -5.0);
    }

    #[test]
    fn flame_dies_after_its_life() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.max_age = 3;
        for _ in 0..3 {
            tick(&mut particle);
        }
        assert!(!particle.dead);
        tick(&mut particle);
        assert!(particle.dead);
    }

    #[test]
    fn flame_shrinks_with_the_square_of_its_age() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.max_age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 2.0);
        particle.age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 1.0);
    }

    #[test]
    fn flame_light_rises_with_age() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.max_age = 10;
        assert_eq!(particle.light(0, 100, 0.0), (0, 100));
        particle.age = 5;
        assert_eq!(particle.light(0, 100, 0.0), (120, 100));
        particle.age = 10;
        assert_eq!(particle.light(100, 100, 0.0), (240, 100));
    }
}
