// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pieces of `onUpdate` and `renderParticle` that several families
//! repeat. None of these families calls `EntityFX.onUpdate`, so each one has
//! its own tick built from these steps.

use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockState;

use crate::particles::{Particle, Texture};
use crate::shapes::BlockSource;

/// The extra friction of a particle that rests on the ground, MCP
/// `EntityHeartFX.java:64` (`0.7F` widened to a double).
const GROUND_FRICTION: f64 = 0.699_999_988_079_071;
/// The speed-up of a particle that did not rise or fall, MCP
/// `EntityHeartFX.java:57`.
const LEVEL_BOOST: f64 = 1.1;
/// How fast the quad grows from nothing, MCP `EntityHeartFX.java:37`: full
/// size after `1 / 32` of the life.
const GROWTH_RATE: f32 = 32.0;

/// Damping of the random start motion of the heart and the note, MCP
/// `EntityHeartFX.java:21` (`0.01F` widened to a double).
pub(super) const SLOW_START: f64 = 0.009_999_999_776_482_582;

/// A world without blocks, for the constructors that run `onUpdate` once
/// before the particle meets the real world (`EntityCrit2FX`).
pub(super) struct OpenAir;

impl BlockSource for OpenAir {
    fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
        BlockState::AIR
    }
}

/// The start of every `onUpdate` of these classes: remember the position and
/// age, dying when the age before the increment reached the life (`if
/// (this.particleAge++ >= this.particleMaxAge) setDead()`, e.g. MCP
/// `EntityHeartFX.java:50`). The rest of the tick still runs, as in Java.
pub(super) fn begin_tick(particle: &mut Particle) {
    particle.previous_position = particle.position;
    if particle.age >= particle.max_age {
        particle.dead = true;
    }
    particle.age += 1;
}

/// `moveEntity(motion)` and then, when the particle neither rose nor fell,
/// the `1.1` speed-up of its horizontal motion (MCP `EntityHeartFX.java:55-60`).
pub(super) fn move_and_boost<S: BlockSource + ?Sized>(particle: &mut Particle, world: &S) {
    particle.move_with_collision(world);
    if particle.position.y == particle.previous_position.y {
        particle.motion.x *= LEVEL_BOOST;
        particle.motion.z *= LEVEL_BOOST;
    }
}

/// Multiplies the whole motion by `drag`, then the horizontal motion by the
/// ground friction when the particle rests on the ground (MCP
/// `EntityHeartFX.java:62-70`).
pub(super) fn drag_and_rest(particle: &mut Particle, drag: f64) {
    particle.motion *= drag;
    if particle.on_ground {
        particle.motion.x *= GROUND_FRICTION;
        particle.motion.z *= GROUND_FRICTION;
    }
}

/// The factor of the `renderParticle` overrides: `clamp((age + partial) /
/// life * 32, 0, 1)`, so the quad grows to its size over the first 1/32 of the
/// life (MCP `EntityHeartFX.java:36-38`).
pub(super) fn growth(particle: &Particle, partial_ticks: f32) -> f32 {
    let progress = (particle.age as f32 + partial_ticks) / particle.max_age as f32;
    (progress * GROWTH_RATE).clamp(0.0, 1.0)
}

/// `setParticleTextureIndex(base + 7 - age * 8 / life)`: the cell that steps
/// through eight pictures over the life (MCP `EntityCloudFX.java:55`).
///
/// The result is negative only on the tick the particle dies (the age has
/// then passed the life), when nothing is drawn; it saturates to cell 0.
pub(super) fn step_cell(base: i32, particle: &Particle) -> Texture {
    let step = (particle.age * 8 / particle.max_age) as i32;
    cell(base + 7 - step)
}

/// The texture of cell `index` of `particles.png`.
pub(super) fn cell(index: i32) -> Texture {
    Texture::Cell {
        index: u8::try_from(index).unwrap_or(0),
    }
}

/// A grey of `1 - random * 0.3`, the colour of the cloud and snow shovel
/// particles (MCP `EntityCloudFX.java:24`).
pub(super) fn light_grey(rng: &mut JavaRandom) -> [f32; 3] {
    let grey = 1.0 - (rng.next_double() * 0.300_000_011_920_928_96) as f32;
    [grey; 3]
}
