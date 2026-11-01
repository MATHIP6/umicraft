// SPDX-License-Identifier: MIT OR Apache-2.0

//! The water-bound particles: bubbles, suspended specks and the fish wake.
//!
//! None of their `onUpdate` overrides calls the `EntityFX` one: they remember
//! the position, move, damp the motion and count their life down on their own.
//! MCP `EntityBubbleFX`, `EntitySuspendFX` and `EntityFishWakeFX`.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::shared::{count_down, is_in_water, random_life, random_offset};
use super::{Behaviour, TickContext};
use crate::particles::particle::{Particle, Texture};
use crate::shapes::BlockSource;

/// The bubble's share of the packet speed, MCP `EntityBubbleFX.java:19`
/// (`0.2F` widened).
const BUBBLE_SPEED: f64 = 0.200_000_002_980_232_24;
/// The bubble's random motion, MCP `EntityBubbleFX.java:19` (`0.02F` widened).
const BUBBLE_SPREAD: f64 = 0.019_999_999_552_965_164;
/// The upward push of a bubble per tick, MCP `EntityBubbleFX.java:30`.
const BUBBLE_BUOYANCY: f64 = 0.002;
/// The damping of a bubble per tick, MCP `EntityBubbleFX.java:32` (`0.85F`
/// widened).
const BUBBLE_DRAG: f64 = 0.850_000_023_841_857_9;
/// The damping of a fish wake per tick, MCP `EntityFishWakeFX.java:40`
/// (`0.98F` widened).
const WAKE_DRAG: f64 = 0.980_000_019_073_486_3;
/// The first cell of the wake animation in `particles.png`, MCP
/// `EntityFishWakeFX.java:19`.
const WAKE_FIRST_CELL: u8 = 19;
/// The life a wake is counted from to get its growth, MCP
/// `EntityFishWakeFX.java:42`.
const WAKE_GROWTH_BASE: i32 = 60;

/// `EntityBubbleFX.Factory`: a bubble rising from `position`, thrown at 0.2
/// of `speed` (MCP `EntityBubbleFX.java:12-23`).
#[must_use]
pub(super) fn bubble(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    particle.color = [1.0; 3];
    particle.texture = Texture::Cell { index: 32 };
    particle.size = 0.02;
    particle.scale *= rng.next_float() * 0.6 + 0.2;
    particle.motion = DVec3::new(
        speed.x * BUBBLE_SPEED + random_offset(rng, BUBBLE_SPREAD),
        speed.y * BUBBLE_SPEED + random_offset(rng, BUBBLE_SPREAD),
        speed.z * BUBBLE_SPEED + random_offset(rng, BUBBLE_SPREAD),
    );
    particle.max_age = random_life(rng, 8.0);
    particle.behaviour = Behaviour::Bubble;
    particle
}

/// `EntityBubbleFX.onUpdate`: floats up, slows down and pops as soon as it
/// leaves the water.
pub(super) fn update_bubble<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    particle.motion.y += BUBBLE_BUOYANCY;
    particle.move_with_collision(context.world);
    particle.motion *= BUBBLE_DRAG;
    if !is_in_water(context.world, particle.position) {
        particle.dead = true;
    }
    count_down(particle);
}

/// `EntitySuspendFX.Factory`: a still, bluish speck 0.125 below `position`
/// (MCP `EntitySuspendFX.java:12-25`).
///
/// The packet speed is multiplied by 0 in the reference, so the speck starts
/// at rest and `speed` only feeds the base constructor.
#[must_use]
pub(super) fn suspended(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let lowered = position - DVec3::new(0.0, 0.125, 0.0);
    let mut particle = Particle::with_velocity(lowered, speed, rng);
    particle.color = [0.4, 0.4, 0.7];
    particle.texture = Texture::Cell { index: 0 };
    particle.size = 0.01;
    particle.scale *= rng.next_float() * 0.6 + 0.2;
    particle.motion = DVec3::ZERO;
    particle.max_age = random_life(rng, 16.0);
    particle.behaviour = Behaviour::Suspended;
    particle
}

/// `EntitySuspendFX.onUpdate`: drifts at constant speed (none, as built)
/// while it stays in water.
pub(super) fn update_suspended<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    particle.move_with_collision(context.world);
    if !is_in_water(context.world, particle.position) {
        particle.dead = true;
    }
    count_down(particle);
}

/// `EntityFishWakeFX.Factory`: a wake ripple carried by `speed` (MCP
/// `EntityFishWakeFX.java:12-26`).
///
/// The constructor first draws a velocity and an upward kick, then replaces
/// both with the packet speed; only the life draw survives.
#[must_use]
pub(super) fn wake(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.color = [1.0; 3];
    particle.texture = Texture::Cell {
        index: WAKE_FIRST_CELL,
    };
    particle.size = 0.01;
    particle.max_age = random_life(rng, 8.0);
    particle.gravity = 0.0;
    particle.motion = speed;
    particle.behaviour = Behaviour::Wake;
    particle
}

/// `EntityFishWakeFX.onUpdate`: moves, damps, and grows by `0.001` per tick
/// of its life while the picture cycles through four cells.
pub(super) fn update_wake<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    particle.motion.y -= f64::from(particle.gravity);
    particle.move_with_collision(context.world);
    particle.motion *= WAKE_DRAG;
    let elapsed = WAKE_GROWTH_BASE - i32::try_from(particle.max_age).unwrap_or(i32::MAX);
    particle.size = f64::from(elapsed as f32 * 0.001);
    particle.texture = Texture::Cell {
        index: WAKE_FIRST_CELL + elapsed.rem_euclid(4) as u8,
    };
    count_down(particle);
}

#[cfg(test)]
mod tests {
    use super::super::shared::Layered;
    use super::*;
    use umicraft_world::BlockState;

    fn water() -> Layered {
        Layered {
            top: i32::MAX,
            state: BlockState::new(9, 0),
        }
    }

    fn air() -> Layered {
        Layered {
            top: 0,
            state: BlockState::AIR,
        }
    }

    fn tick<S: BlockSource>(particle: &mut Particle, world: &S, rng: &mut JavaRandom) {
        let mut spawned = Vec::new();
        let mut context = TickContext {
            world,
            rng,
            spawned: &mut spawned,
        };
        let behaviour = particle.behaviour;
        behaviour.update(particle, &mut context);
    }

    #[test]
    fn a_bubble_starts_small_and_lives_8_to_40_ticks() {
        let mut rng = JavaRandom::new(5);
        for _ in 0..200 {
            let bubble = bubble(DVec3::ZERO, DVec3::ZERO, &mut rng);
            assert_eq!(bubble.texture, Texture::Cell { index: 32 });
            assert_eq!(bubble.size, 0.02);
            assert!((8..=40).contains(&bubble.max_age));
            assert!(bubble.motion.abs().max_element() <= BUBBLE_SPREAD);
        }
    }

    #[test]
    fn a_bubble_rises_in_water() {
        let mut rng = JavaRandom::new(5);
        let mut bubble = bubble(DVec3::new(0.5, 4.0, 0.5), DVec3::ZERO, &mut rng);
        bubble.motion = DVec3::ZERO;
        tick(&mut bubble, &water(), &mut rng);
        assert!(bubble.position.y > 4.0);
        assert!(!bubble.dead);
    }

    #[test]
    fn a_bubble_dies_out_of_the_water() {
        let mut rng = JavaRandom::new(5);
        let mut bubble = bubble(DVec3::new(0.5, 4.0, 0.5), DVec3::ZERO, &mut rng);
        tick(&mut bubble, &air(), &mut rng);
        assert!(bubble.dead);
    }

    #[test]
    fn a_bubble_dies_when_its_life_runs_out() {
        let mut rng = JavaRandom::new(5);
        let mut bubble = bubble(DVec3::new(0.5, 4.0, 0.5), DVec3::ZERO, &mut rng);
        bubble.max_age = 3;
        for _ in 0..3 {
            tick(&mut bubble, &water(), &mut rng);
            assert!(!bubble.dead);
        }
        tick(&mut bubble, &water(), &mut rng);
        assert!(bubble.dead);
    }

    #[test]
    fn a_suspended_speck_is_blue_still_and_lowered() {
        let mut rng = JavaRandom::new(9);
        let mut speck = suspended(DVec3::new(0.5, 4.0, 0.5), DVec3::ONE, &mut rng);
        assert_eq!(speck.color, [0.4, 0.4, 0.7]);
        assert_eq!(speck.position.y, 3.875);
        assert!((16..=80).contains(&speck.max_age));
        tick(&mut speck, &water(), &mut rng);
        assert_eq!(speck.position, DVec3::new(0.5, 3.875, 0.5));
    }

    #[test]
    fn a_suspended_speck_dies_out_of_the_water() {
        let mut rng = JavaRandom::new(9);
        let mut speck = suspended(DVec3::new(0.5, 4.0, 0.5), DVec3::ZERO, &mut rng);
        tick(&mut speck, &air(), &mut rng);
        assert!(speck.dead);
    }

    #[test]
    fn a_wake_takes_the_packet_speed_and_keeps_it() {
        let mut rng = JavaRandom::new(2);
        let speed = DVec3::new(0.01, 0.0, -0.02);
        let mut wake = wake(DVec3::new(0.5, 4.0, 0.5), speed, &mut rng);
        assert_eq!(wake.motion, speed);
        tick(&mut wake, &water(), &mut rng);
        assert!((wake.motion.x - 0.01 * WAKE_DRAG).abs() < 1e-12);
    }

    #[test]
    fn a_wake_grows_and_cycles_its_picture() {
        let mut rng = JavaRandom::new(2);
        let mut wake = wake(DVec3::new(0.5, 4.0, 0.5), DVec3::ZERO, &mut rng);
        wake.max_age = 40;
        tick(&mut wake, &water(), &mut rng);
        // elapsed = 60 - 40 = 20 and 20 % 4 = 0.
        assert!((wake.size - 0.02).abs() < 1e-6);
        assert_eq!(wake.texture, Texture::Cell { index: 19 });
        tick(&mut wake, &water(), &mut rng);
        assert_eq!(wake.texture, Texture::Cell { index: 20 });
        assert_eq!(wake.max_age, 38);
    }
}
