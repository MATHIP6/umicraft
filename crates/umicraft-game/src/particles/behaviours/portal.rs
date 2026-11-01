// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portal: `EntityPortalFX` (nether portal blocks, endermen, ender pearls).
//!
//! A portal particle does not fly: each tick it is placed on a curve from the
//! point where it was created, `origin + speed * f` horizontally, where `f`
//! runs from 1 down to 0 (so the particle ends up back at its origin), with a
//! small vertical lift of `1 - f`. It ignores the blocks and glows with the
//! fourth power of its age ([`Lighting::PortalGlow`]).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::Behaviour;
use crate::particles::particle::{Lighting, Particle, Texture};

/// The life is `40 + int(random * 10)` ticks, MCP `EntityPortalFX.java:31`.
const MIN_LIFE: u32 = 40;
/// The number of purple cells (0 to 7) picked at random, MCP line 33.
const CELLS: f64 = 8.0;

/// A portal particle at `position` that travels `speed` away (MCP
/// `EntityPortalFX` constructor, lines 17-34): the motion is exactly the
/// speed, the scale is `0.5..0.7`, the colour a purple `(0.9, 0.3, 1.0) * f`
/// for a brightness `f` in `0.4..1.0`.
#[must_use]
pub(super) fn create(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    particle.motion = speed;
    let brightness = rng.next_float() * 0.6 + 0.4;
    particle.scale = rng.next_float() * 0.2 + 0.5;
    particle.color = [brightness * 0.9, brightness * 0.3, brightness];
    particle.max_age = (rng.next_double() * 10.0) as u32 + MIN_LIFE;
    particle.no_clip = true;
    particle.texture = Texture::Cell {
        index: (rng.next_double() * CELLS) as u8,
    };
    particle.lighting = Lighting::PortalGlow;
    particle.behaviour = Behaviour::Portal {
        origin: position,
        base_scale: particle.scale,
    };
    particle
}

/// `EntityPortalFX.onUpdate`: the position follows the curve first, then the
/// particle ages. It does not call the common update, so it never falls.
pub(super) fn update(particle: &mut Particle, origin: DVec3) {
    particle.remember_position();
    let progress = particle.age as f32 / particle.max_age as f32;
    let along = f64::from(1.0 - (-progress + progress * progress * 2.0));
    particle.position = DVec3::new(
        origin.x + particle.motion.x * along,
        origin.y + particle.motion.y * along + (1.0 - along),
        origin.z + particle.motion.z * along,
    );
    particle.grow_older();
}

/// The scale to draw with: `base * (1 - (1 - f)^2)` (`renderParticle`, MCP
/// `EntityPortalFX.java:38-46`), growing quickly then settling.
#[must_use]
pub(super) fn render_scale(base_scale: f32, particle: &Particle, partial_ticks: f32) -> f32 {
    let remaining = 1.0 - (particle.age as f32 + partial_ticks) / particle.max_age as f32;
    base_scale * (1.0 - remaining * remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: DVec3 = DVec3::new(10.0, 64.0, 10.0);

    fn make(speed: DVec3, seed: u64) -> Particle {
        create(ORIGIN, speed, &mut JavaRandom::new(seed))
    }

    fn tick(particle: &mut Particle) {
        let Behaviour::Portal { origin, .. } = particle.behaviour else {
            panic!("not a portal particle");
        };
        update(particle, origin);
    }

    #[test]
    fn constructor_values_stay_in_the_reference_ranges() {
        for seed in 0..200 {
            let particle = make(DVec3::new(1.0, 0.0, -1.0), seed);
            assert!((40..=49).contains(&particle.max_age));
            assert!((0.5..=0.7).contains(&particle.scale));
            let [red, green, blue] = particle.color;
            assert!((0.4..=1.0).contains(&blue));
            assert!((red - blue * 0.9).abs() < 1e-6 && (green - blue * 0.3).abs() < 1e-6);
            let Texture::Cell { index } = particle.texture else {
                panic!("not a cell");
            };
            assert!(index < 8);
            assert_eq!(particle.motion, DVec3::new(1.0, 0.0, -1.0));
        }
    }

    #[test]
    fn first_tick_puts_the_particle_at_the_far_end_of_the_curve() {
        let mut particle = make(DVec3::new(2.0, 1.0, -2.0), 1);
        tick(&mut particle);
        // age 0: f = 1 - 0 = 1, so origin + motion with no extra lift.
        assert_eq!(particle.position, ORIGIN + DVec3::new(2.0, 1.0, -2.0));
        assert_eq!(particle.previous_position, ORIGIN);
    }

    #[test]
    fn particle_comes_back_towards_its_origin() {
        let mut particle = make(DVec3::new(2.0, 0.0, 0.0), 1);
        particle.max_age = 10;
        for _ in 0..10 {
            tick(&mut particle);
        }
        // Before the last tick the age was 9: f = 1 - (-0.9 + 1.62) = 0.28.
        assert!((particle.position.x - (ORIGIN.x + 2.0 * 0.28)).abs() < 1e-6);
        assert!(!particle.dead);
        tick(&mut particle);
        assert!(particle.dead);
        // Age 10: f = 1 - (-1 + 2) = 0, the particle is at its origin, lifted by 1.
        assert!((particle.position - (ORIGIN + DVec3::Y)).length() < 1e-6);
    }

    #[test]
    fn size_grows_from_nothing() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.max_age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 0.0);
        particle.age = 10;
        assert_eq!(render_scale(2.0, &particle, 0.0), 2.0);
    }

    #[test]
    fn sky_light_rises_with_the_fourth_power_of_the_age() {
        let mut particle = make(DVec3::ZERO, 1);
        particle.max_age = 10;
        particle.age = 5;
        // (0.5)^4 * 240 = 15.
        assert_eq!(particle.light(80, 0, 0.9), (80, 15));
        particle.age = 10;
        assert_eq!(particle.light(80, 100, 0.0), (80, 240));
    }
}
