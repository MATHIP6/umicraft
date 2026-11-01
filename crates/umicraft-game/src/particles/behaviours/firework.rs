// SPDX-License-Identifier: MIT OR Apache-2.0

//! The firework spark (MCP `EntityFirework.SparkFX`): a bright speck that
//! slows, fades to a second colour, optionally twinkles and leaves a trail.
//! The shells that throw the sparks (`StarterFX`) need the firework NBT and
//! are not ported yet; they build sparks with [`spark`].

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, step_cell};
use super::{Behaviour, TickContext};
use crate::particles::particle::{Lighting, Particle};
use crate::shapes::BlockSource;

/// First of the eight fading cells, MCP `EntityFirework.java:56`.
const BASE_CELL: i32 = 160;
/// Opacity at birth, MCP `EntityFirework.java:21` (`0.99F` widened); it keeps
/// the spark in the translucent pass.
const BIRTH_ALPHA: f32 = 0.99;
/// Downward pull per tick, MCP line 133.
const SINK: f64 = 0.004;
/// Drag per tick, MCP line 134 (`0.91F` widened).
const DRAG: f64 = 0.910_000_026_226_043_7;
/// Ground friction, MCP line 139.
const GROUND_FRICTION: f64 = 0.699_999_988_079_071;
/// Share of the way to the fade colour per tick, MCP line 125.
const FADE_RATE: f32 = 0.2;

/// What distinguishes a spark: its colours and the two effect flags
/// (`setColour`, `setFadeColour`, `setTrail`, `setTwinkle`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SparkStyle {
    /// Colour at birth.
    pub color: [f32; 3],
    /// Colour it drifts to in the second half of its life, if any.
    pub fade: Option<[f32; 3]>,
    /// Whether it drops child sparks while in the first half of its life.
    pub trail: bool,
    /// Whether it blinks out in its last two thirds.
    pub twinkle: bool,
}

impl Default for SparkStyle {
    /// The spark of a `fireworksSpark` packet: white, no fade, no effects.
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            fade: None,
            trail: false,
            twinkle: false,
        }
    }
}

/// A spark moving at `motion` (MCP `SparkFX` constructor): three quarters of
/// the base scale, a life of 48 to 59 ticks, fully lit, stopped by blocks.
#[must_use]
pub fn spark(position: DVec3, motion: DVec3, style: SparkStyle, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::new(position, rng);
    particle.motion = motion;
    particle.scale *= 0.75;
    particle.max_age = 48 + rng.next_int(12);
    particle.alpha = BIRTH_ALPHA;
    particle.color = style.color;
    particle.lighting = Lighting::FullBright;
    particle.behaviour = Behaviour::Spark(style);
    particle
}

/// `SparkFX.onUpdate`: after half its life the opacity falls linearly and
/// the colour moves towards the fade colour; the cell steps through eight
/// pictures; it sinks, moves, slows and, while young, may drop a trail spark.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    style: SparkStyle,
    context: &mut TickContext<'_, S>,
) {
    begin_tick(particle);
    let half = particle.max_age / 2;
    if particle.age > half {
        particle.alpha = 1.0 - (particle.age - half) as f32 / particle.max_age as f32;
        if let Some(fade) = style.fade {
            for (channel, target) in particle.color.iter_mut().zip(fade) {
                *channel += (target - *channel) * FADE_RATE;
            }
        }
    }
    particle.texture = step_cell(BASE_CELL, particle);
    particle.motion.y -= SINK;
    particle.move_with_collision(context.world);
    particle.motion *= DRAG;
    if particle.on_ground {
        particle.motion.x *= GROUND_FRICTION;
        particle.motion.z *= GROUND_FRICTION;
    }
    if style.trail && particle.age < half && (particle.age + particle.max_age).is_multiple_of(2) {
        let child = trail_spark(particle, style, context.rng);
        context.spawned.push(child);
    }
}

/// The spark a trailing one drops: still, in the colour reached so far, born
/// already half-aged so it only fades.
fn trail_spark(parent: &Particle, style: SparkStyle, rng: &mut JavaRandom) -> Particle {
    let child_style = SparkStyle {
        color: parent.color,
        trail: false,
        ..style
    };
    let mut child = spark(parent.position, DVec3::ZERO, child_style, rng);
    child.age = child.max_age / 2;
    child
}

/// The scale to draw: a twinkling spark disappears (scale 0) during the
/// "off" thirds of its last two thirds, MCP `SparkFX.renderParticle`.
pub(super) fn render_scale(style: SparkStyle, particle: &Particle) -> f32 {
    let visible = !style.twinkle
        || particle.age < particle.max_age / 3
        || ((particle.age + particle.max_age) / 3).is_multiple_of(2);
    if visible {
        particle.scale
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{Layer, ParticleEngine};
    use umicraft_world::World;

    fn engine_with(style: SparkStyle) -> (ParticleEngine, u32) {
        let mut engine = ParticleEngine::new(1);
        let particle = spark(
            DVec3::new(0.0, 80.0, 0.0),
            DVec3::ZERO,
            style,
            &mut JavaRandom::new(4),
        );
        let life = particle.max_age;
        engine.add(particle);
        (engine, life)
    }

    #[test]
    fn a_spark_lives_48_to_59_ticks() {
        let (engine, life) = engine_with(SparkStyle::default());
        assert!((48..60).contains(&life));
        assert_eq!(engine.particles(Layer::Misc, true)[0].alpha, BIRTH_ALPHA);
    }

    #[test]
    fn it_fades_out_and_blends_to_the_fade_colour_in_the_second_half() {
        let style = SparkStyle {
            fade: Some([0.0, 0.0, 1.0]),
            ..SparkStyle::default()
        };
        let (mut engine, _) = engine_with(style);
        for _ in 0..40 {
            engine.tick(&World::new(0));
        }
        let aged = &engine.particles(Layer::Misc, true)[0];
        assert!(aged.alpha < 1.0 && aged.alpha > 0.0);
        assert!(aged.color[0] < 0.1 && aged.color[2] > 0.9);
    }

    #[test]
    fn a_trailing_spark_drops_children_only_in_its_first_half() {
        let style = SparkStyle {
            trail: true,
            ..SparkStyle::default()
        };
        let (mut engine, life) = engine_with(style);
        for _ in 0..life / 2 {
            engine.tick(&World::new(0));
        }
        let after_first_half = engine.len();
        assert!(after_first_half > 10, "{after_first_half}");
        // Children are born half-aged and never trail: nothing new appears
        // until the parent dies and the children fade.
        engine.tick(&World::new(0));
        assert_eq!(engine.len(), after_first_half);
    }

    #[test]
    fn twinkling_hides_the_spark_in_alternate_thirds() {
        let style = SparkStyle {
            twinkle: true,
            ..SparkStyle::default()
        };
        let (engine, _) = engine_with(style);
        let mut particle = engine.particles(Layer::Misc, true)[0].clone();
        particle.max_age = 60;
        particle.age = 10;
        assert_eq!(
            particle.behaviour.render_scale(&particle, 0.0),
            particle.scale
        );
        // (33 + 60) / 3 = 31 is odd: hidden. (30 + 60) / 3 = 30: shown.
        particle.age = 33;
        assert_eq!(particle.behaviour.render_scale(&particle, 0.0), 0.0);
        particle.age = 30;
        assert_eq!(
            particle.behaviour.render_scale(&particle, 0.0),
            particle.scale
        );
    }
}
