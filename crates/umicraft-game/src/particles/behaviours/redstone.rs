// SPDX-License-Identifier: MIT OR Apache-2.0

//! Redstone dust: `EntityReddustFX`.
//!
//! The same fade and growth as the smoke (see [`super::smoke`]) but tinted by
//! the packet: the three speed floats are the red, green and blue of the dust,
//! not a velocity.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::{random_life, Behaviour, TickContext};
use crate::particles::particle::Particle;
use crate::shapes::BlockSource;

/// The share of the random velocity kept, MCP `EntityReddustFX.java:23` (`0.1F`
/// widened).
const VELOCITY_KEPT: f64 = 0.100_000_001_490_116_12;
/// The spread of the colour jitter, MCP `EntityReddustFX.java:34` (`0.2F`
/// widened).
const COLOUR_JITTER: f64 = 0.200_000_002_980_232_24;

/// The dust of `Factory.getEntityFX` (MCP `EntityReddustFX.java:88`): `tint`
/// is the red, green and blue of the packet's speed. A red of 0 is read as 1,
/// so a zero packet gives the classic red dust (MCP line 29). The size factor
/// is the 1.0 of the short constructor.
#[must_use]
pub(super) fn create(position: DVec3, tint: [f32; 3], rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion *= VELOCITY_KEPT;
    let red = if tint[0] == 0.0 { 1.0 } else { tint[0] };
    let brightness = rng.next_double() as f32 * 0.4 + 0.6;
    let mut channel =
        |factor: f32| ((rng.next_double() * COLOUR_JITTER) as f32 + 0.8) * factor * brightness;
    particle.color = [channel(red), channel(tint[1]), channel(tint[2])];
    particle.scale *= 0.75;
    particle.max_age = random_life(8.0, rng);
    particle.behaviour = Behaviour::Redstone {
        base_scale: particle.scale,
    };
    particle
}

/// `EntityReddustFX.onUpdate`: the smoke tick without the upward push.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    super::smoke::drift(particle, context.world, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::particle::Texture;
    use umicraft_world::World;

    fn make(tint: [f32; 3], seed: u64) -> Particle {
        create(
            DVec3::new(0.5, 100.0, 0.5),
            tint,
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
    fn zero_red_reads_as_full_red() {
        for seed in 0..50 {
            let [red, green, blue] = make([0.0, 0.0, 0.0], seed).color;
            // 0.8..1.0 jitter times 0.6..1.0 brightness.
            assert!((0.48..=1.0).contains(&red), "{red}");
            assert_eq!((green, blue), (0.0, 0.0));
        }
    }

    #[test]
    fn colour_follows_the_packet_tint() {
        for seed in 0..50 {
            let [red, green, blue] = make([1.0, 0.5, 0.25], seed).color;
            assert!((0.48..=1.0).contains(&red));
            assert!((0.24..=0.5).contains(&green));
            assert!((0.12..=0.25).contains(&blue));
        }
    }

    #[test]
    fn scale_is_three_quarters_of_the_base_draw() {
        let particle = make([1.0, 0.0, 0.0], 1);
        assert!((0.75..=1.5).contains(&particle.scale));
    }

    #[test]
    fn dust_fades_and_dies() {
        let mut particle = make([1.0, 0.0, 0.0], 4);
        particle.max_age = 8;
        for _ in 0..8 {
            tick(&mut particle);
        }
        assert!(!particle.dead);
        assert_eq!(particle.texture, Texture::Cell { index: 0 });
        tick(&mut particle);
        assert!(particle.dead);
    }
}
