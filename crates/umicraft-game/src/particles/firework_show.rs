// SPDX-License-Identifier: MIT OR Apache-2.0

//! The firework shell: `EntityFirework.StarterFX` and its flash.
//!
//! A rocket that explodes (`Entity Status` 17) leaves a shell that lives a few
//! ticks at the burst point. Every second tick it throws the sparks of the
//! next explosion of the rocket's `Fireworks` NBT (a ball, a star, a creeper
//! head, a burst) and a bright flash, and it plays the blast sound once. The
//! sparks themselves are [`Behaviour::Spark`] particles.
//!
//! Source: MCP `net/minecraft/client/particle/EntityFirework.java` (`StarterFX`
//! lines 187-346, `OverlayFX` lines 24-52) and `WorldClient.makeFireworks`.

use std::f64::consts::PI;

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::behaviours::{firework_spark, Behaviour, SparkStyle};
use super::engine::ParticleEngine;
use super::particle::{Particle, Texture};
use crate::nbt::Nbt;
use crate::sound::cue::{CueCategory, SoundCue};

/// The shell lives this long when the rocket carries no explosion (MCP line
/// 195).
const BARE_LIFE: u32 = 8;
/// The extra life of a shell with a flickering explosion, for the twinkle
/// sound (MCP line 211).
const TWINKLE_LIFE: u32 = 15;
/// `ItemDye.dyeColors[0]`: the colour of an explosion without colours.
const DEFAULT_COLOR: i32 = 1_973_019;
/// Squared distance from the viewer from which a blast sounds far, MCP line 307.
const FAR_SQUARED: f64 = 256.0;
/// The life of the flash, MCP `OverlayFX.java:27`.
const FLASH_LIFE: u32 = 4;
/// The opacity of a new flash; `renderParticle` recomputes it each frame.
const FLASH_ALPHA: f32 = 0.6;
/// The part of `particles.png` the flash shows: columns 4 to 7 of rows 2 to 5
/// (`tex(0.25..0.5, 0.125..0.375)`, MCP lines 40-43).
const FLASH_REGION: Texture = Texture::Region {
    min: [0.25, 0.125],
    max: [0.5, 0.375],
};

/// The sparks of a shaped explosion: the vertices of a star (MCP line 280).
const STAR: [[f64; 2]; 6] = [
    [0.0, 1.0],
    [0.3455, 0.309],
    [0.9511, 0.309],
    [0.379_591_836_734_693_9, -0.126_530_612_244_897_9],
    [0.612_244_897_959_183_7, -0.804_081_632_653_061_2],
    [0.0, -0.359_183_673_469_387_7],
];
/// The outline of a creeper face (MCP line 284).
const CREEPER: [[f64; 2]; 12] = [
    [0.0, 0.2],
    [0.2, 0.2],
    [0.2, 0.6],
    [0.6, 0.6],
    [0.6, 0.2],
    [0.2, 0.2],
    [0.2, 0.0],
    [0.4, 0.0],
    [0.4, -0.6],
    [0.2, -0.6],
    [0.2, -0.4],
    [0.0, -0.4],
];

/// One explosion of a rocket (a `Fireworks.Explosions` entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireworkExplosion {
    /// `Type`: 0 small ball, 1 large ball, 2 star, 3 creeper, 4 burst.
    pub shape: i8,
    /// `Trail`: the sparks drop a trail.
    pub trail: bool,
    /// `Flicker`: the sparks twinkle.
    pub flicker: bool,
    /// `Colors` as `0xRRGGBB` (never empty: black when the tag has none).
    pub colors: Vec<i32>,
    /// `FadeColors` as `0xRRGGBB`.
    pub fades: Vec<i32>,
}

impl FireworkExplosion {
    /// The explosions of a `Fireworks` compound (`Explosions`, a list of
    /// compounds). Anything malformed is skipped like a missing tag.
    #[must_use]
    pub fn list_from_nbt(fireworks: &Nbt) -> Vec<Self> {
        let Some(list) = fireworks.get("Explosions").and_then(Nbt::as_list) else {
            return Vec::new();
        };
        list.iter().map(Self::from_nbt).collect()
    }

    fn from_nbt(tag: &Nbt) -> Self {
        let flag = |key| tag.get(key).and_then(Nbt::as_int).is_some_and(|v| v != 0);
        let colors = |key| match tag.get(key) {
            Some(Nbt::IntArray(values)) => values.clone(),
            _ => Vec::new(),
        };
        let mut explosion_colors = colors("Colors");
        if explosion_colors.is_empty() {
            explosion_colors.push(DEFAULT_COLOR);
        }
        Self {
            shape: tag.get("Type").and_then(Nbt::as_int).unwrap_or(0) as i8,
            trail: flag("Trail"),
            flicker: flag("Flicker"),
            colors: explosion_colors,
            fades: colors("FadeColors"),
        }
    }
}

/// A shell waiting to throw its explosions.
#[derive(Debug)]
pub(super) struct FireworkShow {
    position: DVec3,
    motion: DVec3,
    age: u32,
    max_age: u32,
    twinkle: bool,
    far: bool,
    explosions: Vec<FireworkExplosion>,
}

impl FireworkShow {
    /// Whether the shell has finished (`setDead`).
    pub(super) fn finished(&self) -> bool {
        self.age > self.max_age
    }

    /// `StarterFX.onUpdate`: throws the sparks due this tick into `out` and
    /// the sounds into `sounds`.
    pub(super) fn tick(
        &mut self,
        rng: &mut JavaRandom,
        out: &mut Vec<Particle>,
        sounds: &mut Vec<SoundCue>,
    ) {
        if self.age == 0 && !self.explosions.is_empty() {
            sounds.push(self.blast_sound(rng));
        }
        if self.age.is_multiple_of(2) {
            if let Some(explosion) = self.explosions.get((self.age / 2) as usize) {
                self.throw(explosion, rng, out);
            }
        }
        self.age += 1;
        if self.age > self.max_age && self.twinkle {
            sounds.push(self.twinkle_sound(rng));
        }
    }

    fn blast_sound(&self, rng: &mut JavaRandom) -> SoundCue {
        let large = self.explosions.len() >= 3 || self.explosions.iter().any(|e| e.shape == 1);
        let name = match (large, self.far) {
            (true, true) => "fireworks.largeBlast_far",
            (true, false) => "fireworks.largeBlast",
            (false, true) => "fireworks.blast_far",
            (false, false) => "fireworks.blast",
        };
        self.cue(name, 0.95 + rng.next_float() * 0.1)
    }

    fn twinkle_sound(&self, rng: &mut JavaRandom) -> SoundCue {
        let name = if self.far {
            "fireworks.twinkle_far"
        } else {
            "fireworks.twinkle"
        };
        self.cue(name, 0.9 + rng.next_float() * 0.15)
    }

    fn cue(&self, name: &'static str, pitch: f32) -> SoundCue {
        SoundCue {
            name: name.into(),
            pos: self.position.to_array(),
            volume: 20.0,
            pitch,
            category: CueCategory::Neutral,
        }
    }

    /// One explosion: its sparks and the flash in its first colour.
    fn throw(&self, explosion: &FireworkExplosion, rng: &mut JavaRandom, out: &mut Vec<Particle>) {
        let mut sparks = Sparks {
            origin: self.position,
            explosion,
            rng,
            out,
        };
        match explosion.shape {
            1 => sparks.ball(0.5, 4),
            2 => sparks.shaped(0.5, &STAR, false),
            3 => sparks.shaped(0.5, &CREEPER, true),
            4 => sparks.burst(self.motion),
            _ => sparks.ball(0.25, 2),
        }
        out.push(flash(self.position, explosion.colors[0], rng));
    }
}

/// The sparks of one explosion under construction.
struct Sparks<'a> {
    origin: DVec3,
    explosion: &'a FireworkExplosion,
    rng: &'a mut JavaRandom,
    out: &'a mut Vec<Particle>,
}

impl Sparks<'_> {
    /// `createParticle`: a spark with a random colour of the explosion and,
    /// when it has fade colours, a random one of those.
    fn spark(&mut self, motion: DVec3) {
        let explosion = self.explosion;
        let color = explosion.colors[self.rng.next_int(explosion.colors.len() as u32) as usize];
        let fade = (!explosion.fades.is_empty())
            .then(|| explosion.fades[self.rng.next_int(explosion.fades.len() as u32) as usize]);
        let style = SparkStyle {
            color: rgb(color),
            fade: fade.map(rgb),
            trail: explosion.trail,
            twinkle: explosion.flicker,
        };
        self.out
            .push(firework_spark(self.origin, motion, style, self.rng));
    }

    /// `createBall`: sparks on the surface of a cube of side `2 * size`,
    /// jittered and scaled onto a sphere.
    fn ball(&mut self, speed: f64, size: i32) {
        for i in -size..=size {
            for j in -size..=size {
                let mut k = -size;
                while k <= size {
                    let x = f64::from(j) + self.jitter();
                    let y = f64::from(i) + self.jitter();
                    let z = f64::from(k) + self.jitter();
                    let length =
                        (x * x + y * y + z * z).sqrt() / speed + self.rng.next_gaussian() * 0.05;
                    self.spark(DVec3::new(x / length, y / length, z / length));
                    // Inside the cube only its two faces along `k` count.
                    if i != -size && i != size && j != -size && j != size {
                        k += size * 2 - 1;
                    }
                    k += 1;
                }
            }
        }
    }

    /// `(nextDouble() - nextDouble()) * 0.5`.
    fn jitter(&mut self) -> f64 {
        (self.rng.next_double() - self.rng.next_double()) * 0.5
    }

    /// `createShaped`: the outline of `shape` swept round three times, mirrored.
    /// `creeper` narrows the sweep (`0.034` against `0.34`).
    fn shaped(&mut self, speed: f64, shape: &[[f64; 2]], creeper: bool) {
        let [first_x, first_y] = shape[0];
        self.spark(DVec3::new(first_x * speed, first_y * speed, 0.0));
        let start = f64::from(self.rng.next_float() * std::f32::consts::PI);
        let sweep = if creeper { 0.034 } else { 0.34 };
        for turn in 0..3 {
            let angle = start + f64::from(turn as f32 * std::f32::consts::PI) * sweep;
            let (mut from_x, mut from_y) = (first_x, first_y);
            for &[to_x, to_y] in &shape[1..] {
                for step in 1..=4 {
                    let t = 0.25 * f64::from(step);
                    let along = (from_x + (to_x - from_x) * t) * speed;
                    let height = (from_y + (to_y - from_y) * t) * speed;
                    let depth = along * angle.sin();
                    let across = along * angle.cos();
                    for mirror in [-1.0, 1.0] {
                        self.spark(DVec3::new(across * mirror, height, depth * mirror));
                    }
                }
                (from_x, from_y) = (to_x, to_y);
            }
        }
    }

    /// `createBurst`: 70 sparks that follow the rocket's motion.
    fn burst(&mut self, motion: DVec3) {
        let drift_x = self.rng.next_gaussian() * 0.05;
        let drift_z = self.rng.next_gaussian() * 0.05;
        for _ in 0..70 {
            let x = motion.x * 0.5 + self.rng.next_gaussian() * 0.15 + drift_x;
            let z = motion.z * 0.5 + self.rng.next_gaussian() * 0.15 + drift_z;
            let y = motion.y * 0.5 + self.rng.next_double() * 0.5;
            self.spark(DVec3::new(x, y, z));
        }
    }
}

/// `setColour`: a `0xRRGGBB` value as three factors.
fn rgb(color: i32) -> [f32; 3] {
    [
        ((color >> 16) & 0xFF) as f32 / 255.0,
        ((color >> 8) & 0xFF) as f32 / 255.0,
        (color & 0xFF) as f32 / 255.0,
    ]
}

/// `OverlayFX`: the flash that fills the sky for four ticks.
fn flash(position: DVec3, color: i32, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::new(position, rng);
    particle.max_age = FLASH_LIFE;
    particle.color = rgb(color);
    particle.alpha = FLASH_ALPHA;
    particle.texture = FLASH_REGION;
    particle.behaviour = Behaviour::FireworkFlash;
    particle
}

/// How the flash grows: `7.1 * sin((age + partial - 1) * pi / 4)` blocks from
/// its centre to an edge (MCP `OverlayFX.renderParticle`, line 33).
#[must_use]
pub(super) fn flash_half_extent(particle: &Particle, partial_ticks: f32) -> f32 {
    7.1 * ((particle.age as f32 + partial_ticks - 1.0) * 0.25 * PI as f32).sin()
}

/// The opacity of the flash: `0.6 - (age + partial - 1) / 8` (line 34).
#[must_use]
pub(super) fn flash_alpha(particle: &Particle, partial_ticks: f32) -> f32 {
    FLASH_ALPHA - (particle.age as f32 + partial_ticks - 1.0) * 0.25 * 0.5
}

impl ParticleEngine {
    /// `WorldClient.makeFireworks`: a shell at `position` that will throw
    /// `explosions` over the next ticks. `viewer` decides whether its sounds
    /// are the far versions (16 blocks or more away).
    pub fn launch_firework(
        &mut self,
        position: DVec3,
        motion: DVec3,
        explosions: Vec<FireworkExplosion>,
        viewer: DVec3,
    ) {
        let mut max_age = BARE_LIFE;
        let mut twinkle = false;
        if !explosions.is_empty() {
            max_age = explosions.len() as u32 * 2 - 1;
            if explosions.iter().any(|explosion| explosion.flicker) {
                twinkle = true;
                max_age += TWINKLE_LIFE;
            }
        }
        self.add_show(FireworkShow {
            position,
            motion,
            age: 0,
            max_age,
            twinkle,
            far: viewer.distance_squared(position) >= FAR_SQUARED,
            explosions,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Layer;
    use umicraft_world::World;

    fn ball(shape: i8, colors: Vec<i32>) -> FireworkExplosion {
        FireworkExplosion {
            shape,
            trail: false,
            flicker: false,
            colors,
            fades: Vec::new(),
        }
    }

    /// Launches one rocket and ticks until its shell is gone, returning the
    /// engine and the number of sparks counted at the end of each tick.
    fn run(explosions: Vec<FireworkExplosion>) -> (ParticleEngine, Vec<usize>) {
        let world = World::new(0);
        let mut engine = ParticleEngine::new(7);
        engine.launch_firework(
            DVec3::new(0.5, 100.0, 0.5),
            DVec3::ZERO,
            explosions,
            DVec3::ZERO,
        );
        let mut counts = Vec::new();
        for _ in 0..6 {
            engine.tick(&world);
            counts.push(engine.len());
        }
        (engine, counts)
    }

    #[test]
    fn a_small_ball_is_the_cube_surface_of_the_reference() {
        // size 2: the 5 x 5 x 5 cube minus its 3 x 3 x 3 inside = 98 sparks,
        // plus the flash.
        let (_, counts) = run(vec![ball(0, vec![0xFF0000])]);
        assert_eq!(counts[0], 98 + 1);
    }

    #[test]
    fn a_large_ball_is_the_surface_of_the_nine_cube() {
        // size 4: 9^3 - 7^3 = 386 sparks.
        let (_, counts) = run(vec![ball(1, vec![0xFF0000])]);
        assert_eq!(counts[0], 386 + 1);
    }

    #[test]
    fn a_burst_throws_seventy_sparks() {
        let (_, counts) = run(vec![ball(4, vec![0x00FF00])]);
        assert_eq!(counts[0], 70 + 1);
    }

    #[test]
    fn a_star_throws_one_spark_then_forty_eight_mirrored_ones_per_turn() {
        // The first spark, then 3 turns x 5 edges x 4 steps x 2 mirrors.
        let (_, counts) = run(vec![ball(2, vec![0x0000FF])]);
        assert_eq!(counts[0], 1 + 3 * 5 * 4 * 2 + 1);
    }

    #[test]
    fn the_explosions_go_off_every_second_tick() {
        let (_, counts) = run(vec![ball(4, vec![1]), ball(4, vec![2])]);
        // Tick 0 throws the first, tick 1 nothing, tick 2 the second.
        assert_eq!(counts[0], 71);
        assert_eq!(counts[1], 71);
        assert_eq!(counts[2], 71 + 71);
    }

    #[test]
    fn the_flash_wears_the_first_colour_and_shrinks_in_four_ticks() {
        let (engine, _) = run(vec![ball(4, vec![0xFF8000])]);
        let flashes: Vec<_> = engine
            .particles(Layer::Misc, true)
            .iter()
            .filter(|particle| particle.behaviour == Behaviour::FireworkFlash)
            .collect();
        // Born at tick 0, it dies after FLASH_LIFE ticks: gone by tick 6.
        assert!(flashes.is_empty());
        let flash = flash(DVec3::ZERO, 0xFF8000, &mut JavaRandom::new(1));
        assert_eq!(flash.color, [1.0, 128.0 / 255.0, 0.0]);
        assert!(flash.alpha < 1.0, "the flash blends, so it is translucent");
        assert!(flash_half_extent(&flash, 3.0) > 6.5, "peak at age 2 of 4");
        assert!(flash_alpha(&flash, 0.0) > flash_alpha(&flash, 3.0));
    }

    #[test]
    fn the_blast_sound_follows_the_size_and_the_distance() {
        let mut engine = ParticleEngine::new(1);
        let at = DVec3::new(100.0, 70.0, 0.0);
        engine.launch_firework(at, DVec3::ZERO, vec![ball(1, vec![1])], DVec3::ZERO);
        engine.launch_firework(
            at,
            DVec3::ZERO,
            vec![ball(0, vec![1])],
            DVec3::new(99.0, 70.0, 0.0),
        );
        engine.tick(&World::new(0));
        let names: Vec<_> = engine
            .drain_sounds()
            .into_iter()
            .map(|cue| cue.name.into_owned())
            .collect();
        assert_eq!(names, ["fireworks.largeBlast_far", "fireworks.blast"]);
    }

    #[test]
    fn a_flickering_rocket_twinkles_after_its_last_explosion() {
        let mut flicker = ball(0, vec![1]);
        flicker.flicker = true;
        let mut engine = ParticleEngine::new(1);
        engine.launch_firework(DVec3::ZERO, DVec3::ZERO, vec![flicker], DVec3::ZERO);
        let world = World::new(0);
        let mut all = Vec::new();
        // 1 explosion: life 1 + 15; the twinkle comes when the age passes it.
        for _ in 0..20 {
            engine.tick(&world);
            all.extend(engine.drain_sounds());
        }
        let names: Vec<_> = all.iter().map(|cue| cue.name.as_ref()).collect();
        assert_eq!(names, ["fireworks.blast", "fireworks.twinkle"]);
    }

    #[test]
    fn a_rocket_without_explosions_is_silent_and_dies_after_eight_ticks() {
        let mut engine = ParticleEngine::new(1);
        engine.launch_firework(DVec3::ZERO, DVec3::ZERO, Vec::new(), DVec3::ZERO);
        let world = World::new(0);
        for _ in 0..12 {
            engine.tick(&world);
        }
        assert!(engine.is_empty());
        assert!(engine.drain_sounds().is_empty());
    }

    #[test]
    fn explosions_are_read_from_the_fireworks_tag() {
        let tag = Nbt::Compound(vec![(
            "Explosions".into(),
            Nbt::List(vec![
                Nbt::Compound(vec![
                    ("Type".into(), Nbt::Byte(2)),
                    ("Trail".into(), Nbt::Byte(1)),
                    ("Colors".into(), Nbt::IntArray(vec![0xFF0000, 0x00FF00])),
                    ("FadeColors".into(), Nbt::IntArray(vec![0x0000FF])),
                ]),
                Nbt::Compound(Vec::new()),
            ]),
        )]);
        let explosions = FireworkExplosion::list_from_nbt(&tag);
        assert_eq!(explosions.len(), 2);
        assert_eq!(explosions[0].shape, 2);
        assert!(explosions[0].trail && !explosions[0].flicker);
        assert_eq!(explosions[0].colors, [0xFF0000, 0x00FF00]);
        assert_eq!(explosions[0].fades, [0x0000FF]);
        // An empty compound is a black small ball.
        assert_eq!(explosions[1].colors, [DEFAULT_COLOR]);
        assert!(FireworkExplosion::list_from_nbt(&Nbt::Compound(Vec::new())).is_empty());
    }
}
