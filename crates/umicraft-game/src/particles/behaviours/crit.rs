// SPDX-License-Identifier: MIT OR Apache-2.0

//! The critical hit stars: `EntityCrit2FX` and its two factories (MCP
//! `client/particle/EntityCrit2FX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, cell, drag_and_rest, OpenAir};
use super::{Behaviour, TickContext};
use crate::particles::{Particle, ParticleKind};
use crate::shapes::BlockSource;

/// The cell of the star, MCP `EntityCrit2FX.java:31`.
const STAR_CELL: i32 = 65;
/// Damping of the random start motion, MCP `EntityCrit2FX.java:19`.
const START_DAMPING: f64 = 0.100_000_001_490_116_12;
/// Share of the packet speed added to the motion, MCP `EntityCrit2FX.java:22`.
const SPEED_SHARE: f64 = 0.4;
/// Per-tick damping of the motion, MCP `EntityCrit2FX.java:68`.
const DRAG: f64 = 0.699_999_988_079_071;
/// Downward pull per tick, MCP `EntityCrit2FX.java:71`.
const SINK: f64 = 0.019_999_999_552_965_164;
/// Per-tick factor of the green channel, MCP `EntityCrit2FX.java:66`.
const GREEN_FADE: f64 = 0.96;
/// Per-tick factor of the blue channel, MCP `EntityCrit2FX.java:67`.
const BLUE_FADE: f64 = 0.9;

/// `IParticleFactory.getEntityFX` of `Crit` and `MagicCrit`. The magic one
/// tints the (already updated) colour to blue-green and shows the next cell
/// (`nextTextureIndexX`). `kind` must be one of the two.
pub(super) fn create(
    kind: ParticleKind,
    position: DVec3,
    speed: DVec3,
    rng: &mut JavaRandom,
) -> Particle {
    let mut particle = spawn(position, speed, rng);
    if kind == ParticleKind::MagicCrit {
        let [red, green, blue] = particle.color;
        particle.color = [red * 0.3, green * 0.8, blue];
        particle.texture = cell(STAR_CELL + 1);
    }
    particle
}

/// The constructor of `EntityCrit2FX` with scale 1: a grey star thrown along
/// the packet speed, living `6 / (0.6 .. 1.4)` ticks.
///
/// The Java constructor ends with `onUpdate()`, so the particle has already
/// aged and moved once when the factory tints it. That first move runs here in
/// open air because the factory has no world; a crit spawned inside a block
/// therefore passes through it for that one step (approximation).
fn spawn(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion = particle.motion * START_DAMPING + speed * SPEED_SHARE;
    let grey = (rng.next_double() * 0.300_000_011_920_928_96 + 0.600_000_023_841_857_9) as f32;
    particle.color = [grey; 3];
    particle.scale *= 0.75;
    particle.max_age = (6.0 / (rng.next_double() * 0.8 + 0.6)) as u32;
    particle.texture = cell(STAR_CELL);
    particle.behaviour = Behaviour::Crit;
    advance(&mut particle, &OpenAir);
    particle
}

/// `EntityCrit2FX.onUpdate`: ages, moves, loses green and blue so the star
/// reddens, slows down and sinks.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    advance(particle, context.world);
}

fn advance<S: BlockSource + ?Sized>(particle: &mut Particle, world: &S) {
    begin_tick(particle);
    particle.move_with_collision(world);
    particle.color[1] = (f64::from(particle.color[1]) * GREEN_FADE) as f32;
    particle.color[2] = (f64::from(particle.color[2]) * BLUE_FADE) as f32;
    drag_and_rest(particle, DRAG);
    particle.motion.y -= SINK;
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
    fn a_new_crit_has_already_run_one_update() {
        for seed in 0..100 {
            let particle = make(ParticleKind::Crit, seed);
            assert_eq!(particle.age, 1);
            // 6 / (0.6 .. 1.4) is 4 to 10.
            assert!((4..=10).contains(&particle.max_age));
            // (1 .. 2) * 0.75.
            assert!((0.75..=1.5).contains(&particle.scale));
            assert_eq!(particle.texture, Texture::Cell { index: 65 });
        }
    }

    #[test]
    fn the_star_starts_grey_and_loses_green_and_blue() {
        for seed in 0..100 {
            let [red, green, blue] = make(ParticleKind::Crit, seed).color;
            assert!((0.6..=0.9).contains(&red));
            // One update has run: green * 0.96, blue * 0.9.
            assert!((green - red * 0.96).abs() < 1e-6);
            assert!((blue - red * 0.9).abs() < 1e-6);
        }
    }

    #[test]
    fn the_magic_crit_is_blue_green_on_the_next_cell() {
        let plain = make(ParticleKind::Crit, 3);
        let magic = make(ParticleKind::MagicCrit, 3);
        assert_eq!(magic.texture, Texture::Cell { index: 66 });
        assert!((magic.color[0] - plain.color[0] * 0.3).abs() < 1e-6);
        assert!((magic.color[1] - plain.color[1] * 0.8).abs() < 1e-6);
        assert_eq!(magic.color[2], plain.color[2]);
    }

    #[test]
    fn the_packet_speed_adds_forty_percent_to_the_motion() {
        let mut rng = JavaRandom::new(5);
        let mut base_rng = JavaRandom::new(5);
        let thrown = spawn(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0), &mut rng);
        let still = spawn(DVec3::ZERO, DVec3::ZERO, &mut base_rng);
        // One update has run, scaling the motion by 0.7 (open air, no ground).
        assert!((thrown.motion.x - still.motion.x - 0.4 * DRAG).abs() < 1e-12);
    }

    #[test]
    fn a_tick_slows_sinks_and_dies_after_the_life() {
        let mut particle = make(ParticleKind::Crit, 9);
        particle.motion = DVec3::new(0.0, 1.0, 0.0);
        particle.max_age = 5;
        tick(&mut particle);
        assert!((particle.motion.y - (DRAG - SINK)).abs() < 1e-12);
        while !particle.dead {
            tick(&mut particle);
        }
        // The age before the increment reached the life of 5: the sixth update.
        assert_eq!(particle.age, 6);
    }
}
