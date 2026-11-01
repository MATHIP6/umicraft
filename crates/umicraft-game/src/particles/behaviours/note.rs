// SPDX-License-Identifier: MIT OR Apache-2.0

//! The music note of a note block: `EntityNoteFX` (MCP
//! `client/particle/EntityNoteFX.java`).

use std::f32::consts::PI;

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, cell, drag_and_rest, move_and_boost, SLOW_START};
use super::{Behaviour, TickContext};
use crate::particles::Particle;
use crate::shapes::BlockSource;

/// The cell of the note, MCP `EntityNoteFX.java:30`.
const NOTE_CELL: i32 = 64;
/// The scale factor of the factory, MCP `EntityNoteFX.java:13`.
const SCALE: f32 = 2.0;
/// The life, MCP `EntityNoteFX.java:28`.
const LIFE: u32 = 6;
/// Upward start speed, MCP `EntityNoteFX.java:22`.
const RISE: f64 = 0.2;
/// Per-tick drag, MCP `EntityNoteFX.java:66`.
const DRAG: f64 = 0.660_000_026_226_043_7;

/// The note colour for the pitch the packet carries in its x speed: three sine
/// waves a third of a turn apart, `sin((pitch + phase) * 2 pi) * 0.65 + 0.35`
/// (MCP `EntityNoteFX.java:23-25`).
///
/// The reference reads `MathHelper.sin`, a 65536-entry table; the exact sine
/// differs from it by under `1e-4`, far below one colour step.
fn note_colour(pitch: f32) -> [f32; 3] {
    let wave = |phase: f32| ((pitch + phase) * PI * 2.0).sin() * 0.65 + 0.35;
    [wave(0.0), wave(0.333_333_34), wave(0.666_666_7)]
}

/// `IParticleFactory.getEntityFX` of `Note`; `speed.x` is the pitch (0 to 1)
/// that colours the note.
pub(super) fn create(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion *= SLOW_START;
    particle.motion.y += RISE;
    particle.color = note_colour(speed.x as f32);
    particle.scale *= 0.75 * SCALE;
    particle.max_age = LIFE;
    particle.texture = cell(NOTE_CELL);
    particle.behaviour = Behaviour::Note;
    particle
}

/// `EntityNoteFX.onUpdate`: the same motion as the heart with a stronger drag.
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

    fn make(pitch: f64) -> Particle {
        let mut rng = JavaRandom::new(2);
        create(
            DVec3::new(0.5, 100.0, 0.5),
            DVec3::new(pitch, 0.0, 0.0),
            &mut rng,
        )
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
    fn constructor_values_match_the_reference() {
        let note = make(0.0);
        assert_eq!(note.max_age, 6);
        assert!((1.5..=3.0).contains(&note.scale));
        assert_eq!(note.texture, Texture::Cell { index: 64 });
        assert!((note.motion.y - 0.2).abs() < 0.003);
    }

    #[test]
    fn pitch_zero_is_a_warm_note() {
        let [red, green, blue] = make(0.0).color;
        assert!((red - 0.35).abs() < 1e-6);
        // sin(2 pi / 3) * 0.65 + 0.35 and sin(4 pi / 3) * 0.65 + 0.35.
        assert!((green - 0.913).abs() < 1e-3);
        assert!((blue + 0.213).abs() < 1e-3);
    }

    #[test]
    fn the_colour_waves_stay_in_range_and_repeat_each_pitch_turn() {
        for step in 0..=20 {
            let pitch = f64::from(step) / 20.0;
            for channel in make(pitch).color {
                assert!((-0.3..=1.0).contains(&channel));
            }
        }
        let start = make(0.0).color;
        let end = make(1.0).color;
        for (a, b) in start.iter().zip(end) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn a_tick_drags_by_the_note_factor() {
        let mut note = make(0.0);
        note.motion = DVec3::new(0.0, 0.2, 0.0);
        tick(&mut note);
        assert!((note.motion.y - 0.2 * DRAG).abs() < 1e-12);
    }

    #[test]
    fn it_lives_six_ticks_and_dies_on_the_next() {
        let mut note = make(0.5);
        for _ in 0..6 {
            tick(&mut note);
            assert!(!note.dead);
        }
        tick(&mut note);
        assert!(note.dead);
    }
}
