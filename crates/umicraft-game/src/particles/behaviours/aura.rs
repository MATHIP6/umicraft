// SPDX-License-Identifier: MIT OR Apache-2.0

//! The drifting specks of mycelium, villagers and the deep water:
//! `EntityAuraFX` and its two factories (MCP
//! `client/particle/EntityAuraFX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::cell;
use super::{Behaviour, TickContext};
use crate::particles::{Particle, ParticleKind};
use crate::shapes::BlockSource;

/// The cell of the grey speck, MCP `EntityAuraFX.java:15`.
const SPECK_CELL: i32 = 0;
/// The cell of the happy villager's star, MCP `EntityAuraFX.java:55`.
const HAPPY_CELL: i32 = 82;
/// The side of the box, MCP `EntityAuraFX.java:16` (`0.02F`).
const BOX_SIZE: f64 = 0.019_999_999_552_965_164;
/// Damping of the start motion, MCP `EntityAuraFX.java:18-20`.
const START_DAMPING: f64 = 0.019_999_999_552_965_164;
/// Per-tick drag, MCP `EntityAuraFX.java:36-38`.
const DRAG: f64 = 0.99;

/// `IParticleFactory.getEntityFX` of `TownAura`, `DepthSuspend` (both a grey
/// speck) and `HappyVillager` (a white star on cell 82); `kind` must be one of
/// the three.
pub(super) fn create(
    kind: ParticleKind,
    position: DVec3,
    speed: DVec3,
    rng: &mut JavaRandom,
) -> Particle {
    let mut particle = spawn(position, speed, rng);
    if kind == ParticleKind::HappyVillager {
        particle.texture = cell(HAPPY_CELL);
        particle.color = [1.0; 3];
    }
    particle
}

/// The constructor of `EntityAuraFX`: a grey of `0.2 .. 0.3`, a tiny box and a
/// life of `20 / (0.2 .. 1.0)` ticks. The particle passes through blocks
/// (`noClip`).
fn spawn(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    let grey = rng.next_float() * 0.1 + 0.2;
    particle.color = [grey; 3];
    particle.texture = cell(SPECK_CELL);
    particle.size = BOX_SIZE;
    particle.scale *= rng.next_float() * 0.6 + 0.5;
    particle.motion *= START_DAMPING;
    particle.max_age = (20.0 / (rng.next_double() * 0.8 + 0.2)) as u32;
    particle.behaviour = Behaviour::Aura;
    particle
}

/// `EntityAuraFX.onUpdate`: drifts through everything with a light drag. The
/// life is counted down in `max_age` itself, as the reference does, and the
/// age never grows.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    _context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    particle.position += particle.motion;
    particle.motion *= DRAG;
    match particle.max_age.checked_sub(1) {
        Some(left) => particle.max_age = left,
        None => particle.dead = true,
    }
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
            let speck = make(ParticleKind::TownAura, seed);
            let grey = speck.color[0];
            assert!((0.2..0.3).contains(&grey));
            assert_eq!(speck.color, [grey; 3]);
            assert_eq!(speck.texture, Texture::Cell { index: 0 });
            // (1 .. 2) * (0.5 .. 1.1).
            assert!((0.5..=2.2).contains(&speck.scale));
            // 20 / (0.2 .. 1.0) is 20 to 100.
            assert!((20..=100).contains(&speck.max_age));
            assert!((speck.size - 0.02).abs() < 1e-6);
        }
    }

    #[test]
    fn kinds_map_to_their_colour_and_cell() {
        let town = make(ParticleKind::TownAura, 6);
        let depth = make(ParticleKind::DepthSuspend, 6);
        assert_eq!(town.color, depth.color);
        assert_eq!(depth.texture, Texture::Cell { index: 0 });
        let happy = make(ParticleKind::HappyVillager, 6);
        assert_eq!(happy.color, [1.0; 3]);
        assert_eq!(happy.texture, Texture::Cell { index: 82 });
    }

    #[test]
    fn the_start_motion_is_damped_to_two_percent() {
        let mut rng = JavaRandom::new(8);
        let thrown = create(
            ParticleKind::TownAura,
            DVec3::ZERO,
            DVec3::new(1.0, 1.0, 1.0),
            &mut rng,
        );
        // |speed + scatter| is renormalised to at most 0.45 * 0.4, plus the lift.
        assert!(thrown.motion.length() < 0.02 * 0.4);
    }

    #[test]
    fn a_tick_moves_through_blocks_and_drags() {
        let mut speck = make(ParticleKind::TownAura, 6);
        speck.position = DVec3::new(0.5, 100.0, 0.5);
        speck.motion = DVec3::new(0.0, -1.0, 0.0);
        tick(&mut speck);
        assert!((speck.position.y - 99.0).abs() < 1e-12);
        assert!((speck.motion.y + 0.99).abs() < 1e-12);
        assert_eq!(speck.age, 0);
    }

    #[test]
    fn the_life_counts_down_and_the_speck_dies_after_it() {
        let mut speck = make(ParticleKind::TownAura, 6);
        speck.max_age = 3;
        for _ in 0..3 {
            tick(&mut speck);
            assert!(!speck.dead);
        }
        tick(&mut speck);
        assert!(speck.dead);
    }
}
