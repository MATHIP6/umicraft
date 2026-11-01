// SPDX-License-Identifier: MIT OR Apache-2.0

//! The potion and enchantment sparkles: `EntitySpellParticleFX` and its five
//! factories (MCP `client/particle/EntitySpellParticleFX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, cell, drag_and_rest, move_and_boost, step_cell};
use super::{Behaviour, TickContext};
use crate::particles::{Particle, ParticleKind};
use crate::shapes::BlockSource;

/// The first cell of the spell pictures, `baseSpellTextureIndex` (MCP
/// `EntitySpellParticleFX.java:14`).
pub(super) const SPELL_CELL: u8 = 128;
/// The first cell of the instant and witch pictures, MCP
/// `EntitySpellParticleFX.java:94` and `:116`.
const INSTANT_CELL: u8 = 144;
/// Vertical damping of the thrown motion, MCP `EntitySpellParticleFX.java:20`.
const LIFT_DAMPING: f64 = 0.200_000_002_980_232_24;
/// Horizontal damping when the packet has no horizontal speed, MCP
/// `EntitySpellParticleFX.java:24-25`.
const STILL_DAMPING: f64 = 0.100_000_001_490_116_12;
/// Upward push per tick, MCP `EntitySpellParticleFX.java:51`.
const BUOYANCY: f64 = 0.004;
/// Air drag per tick, MCP `EntitySpellParticleFX.java:59-61`.
const DRAG: f64 = 0.959_999_978_542_327_9;
/// The opacity of the ambient (beacon, ambient potion) particles, MCP
/// `EntitySpellParticleFX.java:78`.
const AMBIENT_ALPHA: f32 = 0.15;

/// `IParticleFactory.getEntityFX` of the five spell kinds.
///
/// For `MobSpell` and `MobSpellAmbient` the packet's speed is not a velocity
/// but the colour (`setRBGColorF(xSpeed, ySpeed, zSpeed)`); `speed.y` is still
/// the vertical speed, `speed.x` and `speed.z` only decide the horizontal
/// damping (RenderGlobal.doSpawnParticle passes the packet values untouched).
/// `kind` must be one of the spell kinds.
pub(super) fn create(
    kind: ParticleKind,
    position: DVec3,
    speed: DVec3,
    rng: &mut JavaRandom,
) -> Particle {
    let mut particle = spawn(position, speed, rng);
    let colour = [speed.x as f32, speed.y as f32, speed.z as f32];
    match kind {
        ParticleKind::InstantSpell => set_base_cell(&mut particle, INSTANT_CELL),
        ParticleKind::MobSpell => particle.color = colour,
        ParticleKind::MobSpellAmbient => {
            particle.alpha = AMBIENT_ALPHA;
            particle.color = colour;
        }
        ParticleKind::WitchMagic => {
            set_base_cell(&mut particle, INSTANT_CELL);
            let shade = rng.next_float() * 0.5 + 0.35;
            particle.color = [shade, 0.0, shade];
        }
        _ => {}
    }
    particle
}

/// The constructor of `EntitySpellParticleFX`: the velocity constructor with a
/// random horizontal speed in `(-0.5, 0.5]` (`0.5 - random`), a gentle rise
/// and a life of `8 / (0.2 .. 1.0)` ticks.
fn spawn(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let scatter_x = 0.5 - rng.next_double();
    let scatter_z = 0.5 - rng.next_double();
    let mut particle =
        Particle::with_velocity(position, DVec3::new(scatter_x, speed.y, scatter_z), rng);
    particle.motion.y *= LIFT_DAMPING;
    if speed.x == 0.0 && speed.z == 0.0 {
        particle.motion.x *= STILL_DAMPING;
        particle.motion.z *= STILL_DAMPING;
    }
    particle.scale *= 0.75;
    particle.max_age = (8.0 / (rng.next_double() * 0.8 + 0.2)) as u32;
    particle.texture = cell(i32::from(SPELL_CELL));
    particle.behaviour = Behaviour::Spell {
        base_cell: SPELL_CELL,
    };
    particle
}

/// `setBaseSpellTextureIndex`: the cell the animation starts from.
fn set_base_cell(particle: &mut Particle, base_cell: u8) {
    particle.behaviour = Behaviour::Spell { base_cell };
}

/// `EntitySpellParticleFX.onUpdate`: ages, steps through eight pictures,
/// rises slightly, speeds up when level and slows down.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    base_cell: u8,
    context: &mut TickContext<'_, S>,
) {
    begin_tick(particle);
    particle.texture = step_cell(i32::from(base_cell), particle);
    particle.motion.y += BUOYANCY;
    move_and_boost(particle, context.world);
    drag_and_rest(particle, DRAG);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Texture;
    use umicraft_world::World;

    fn make(kind: ParticleKind, speed: DVec3) -> Particle {
        let mut rng = JavaRandom::new(11);
        create(kind, DVec3::new(0.5, 100.0, 0.5), speed, &mut rng)
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
            let mut rng = JavaRandom::new(seed);
            let particle = create(
                ParticleKind::Spell,
                DVec3::ZERO,
                DVec3::new(1.0, 0.0, 1.0),
                &mut rng,
            );
            // 8 / (0.2 .. 1.0) is 8 to 40.
            assert!((8..=40).contains(&particle.max_age));
            // (0.5 .. 1.0) * 2 * 0.75.
            assert!((0.75..=1.5).contains(&particle.scale));
            assert_eq!(particle.color, [1.0; 3]);
            assert_eq!(particle.alpha, 1.0);
        }
    }

    #[test]
    fn a_packet_without_horizontal_speed_is_damped_horizontally() {
        let still = make(ParticleKind::Spell, DVec3::ZERO);
        let moving = make(ParticleKind::Spell, DVec3::new(1.0, 0.0, 1.0));
        // Same seed, so the draws are equal and only the damping differs.
        assert!((still.motion.x - moving.motion.x * STILL_DAMPING).abs() < 1e-12);
        assert!((still.motion.z - moving.motion.z * STILL_DAMPING).abs() < 1e-12);
    }

    #[test]
    fn mob_kinds_take_their_colour_from_the_packet_speed() {
        let speed = DVec3::new(0.25, 0.5, 0.75);
        let mob = make(ParticleKind::MobSpell, speed);
        assert_eq!(mob.color, [0.25, 0.5, 0.75]);
        assert_eq!(mob.alpha, 1.0);
        let ambient = make(ParticleKind::MobSpellAmbient, speed);
        assert_eq!(ambient.color, [0.25, 0.5, 0.75]);
        assert_eq!(ambient.alpha, 0.15);
    }

    #[test]
    fn witch_magic_is_purple_from_the_instant_cells() {
        for seed in 0..50 {
            let mut rng = JavaRandom::new(seed);
            let witch = create(ParticleKind::WitchMagic, DVec3::ZERO, DVec3::ZERO, &mut rng);
            let [red, green, blue] = witch.color;
            assert!((0.35..0.85).contains(&red));
            assert_eq!(red, blue);
            assert_eq!(green, 0.0);
            assert_eq!(witch.behaviour, Behaviour::Spell { base_cell: 144 });
        }
    }

    #[test]
    fn instant_spell_starts_at_cell_144_and_plain_at_128() {
        let mut instant = make(ParticleKind::InstantSpell, DVec3::ZERO);
        let mut plain = make(ParticleKind::Spell, DVec3::ZERO);
        instant.max_age = 16;
        plain.max_age = 16;
        tick(&mut instant);
        tick(&mut plain);
        // Age 1 of 16: 7 - 1 * 8 / 16 = 7.
        assert_eq!(instant.texture, Texture::Cell { index: 151 });
        assert_eq!(plain.texture, Texture::Cell { index: 135 });
    }

    #[test]
    fn the_picture_steps_down_through_the_life_and_the_particle_dies() {
        let mut particle = make(ParticleKind::Spell, DVec3::ZERO);
        particle.max_age = 16;
        for _ in 0..16 {
            tick(&mut particle);
            assert!(!particle.dead);
        }
        // Age 16 of 16: 7 - 16 * 8 / 16 = -1, the last picture of the life.
        assert_eq!(particle.texture, Texture::Cell { index: 127 });
        tick(&mut particle);
        assert!(particle.dead);
    }

    #[test]
    fn a_tick_pushes_up_and_drags() {
        let mut particle = make(ParticleKind::Spell, DVec3::ZERO);
        particle.motion = DVec3::new(0.0, 0.1, 0.0);
        tick(&mut particle);
        assert!((particle.motion.y - (0.1 + 0.004) * DRAG).abs() < 1e-12);
    }
}
