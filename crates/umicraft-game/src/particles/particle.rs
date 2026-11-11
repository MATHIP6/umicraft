// SPDX-License-Identifier: MIT OR Apache-2.0

//! The state every particle shares: `EntityFX` of the 1.8 reference.
//!
//! The behaviour of each particle type (its colour, its growth, its extra
//! motion) lives in `behaviours`; this module owns the constructor draws, the
//! common update and the data the renderer needs.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::behaviours::Behaviour;
use crate::collision::{move_entity, Aabb, MoveOptions};
use crate::shapes::BlockSource;

/// The side of the box of a new particle, MCP `EntityFX.java:51`.
const BOX_SIZE: f64 = 0.2;
/// The scatter of the velocity constructor, MCP `EntityFX.java:70` (`0.4F`
/// widened to a double).
const VELOCITY_SCATTER: f64 = 0.400_000_005_960_464_5;
/// The upward bias of the velocity constructor, MCP `EntityFX.java:77`
/// (`0.1F` widened to a double).
const VELOCITY_LIFT: f64 = 0.100_000_001_490_116_12;
/// The air drag per tick, MCP `EntityFX.java:155` (`0.98F` widened).
const AIR_DRAG: f64 = 0.980_000_019_073_486_3;
/// The ground friction per tick, MCP `EntityFX.java:161` (`0.7F` widened).
const GROUND_FRICTION: f64 = 0.699_999_988_079_071;
/// The gravity step that `particle_gravity` scales, MCP `EntityFX.java:152`.
const GRAVITY_STEP: f64 = 0.04;

/// Which picture the particle samples, and so which pass draws it (MCP
/// `EntityFX.getFXLayer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// A cell of `particles.png` (layer 0).
    Misc,
    /// A sprite of the block atlas (layer 1).
    Terrain,
    /// A sprite of the item atlas (layer 2).
    Items,
    /// The layer 3 of the reference (`renderLitParticles`): the particles
    /// that bring their own picture and quad, the large explosion and the
    /// flat footstep.
    Lit,
}

impl Layer {
    /// The four layers in draw order.
    pub const ALL: [Self; 4] = [Self::Misc, Self::Terrain, Self::Items, Self::Lit];

    /// The index of the layer in the reference (0 to 3).
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }
}

/// How the light of a particle is chosen (MCP `getBrightnessForRender`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lighting {
    /// The world light at the particle (`Entity.getBrightnessForRender`).
    World,
    /// Flame: the block light rises to the maximum as the particle ages
    /// (`EntityFlameFX.getBrightnessForRender`).
    FlameGlow,
    /// Lava spark: the block light is always the maximum
    /// (`EntityLavaFX.getBrightnessForRender`).
    LavaFull,
    /// Portal: the sky light rises to the maximum, with the fourth power of
    /// the age (`EntityPortalFX.getBrightnessForRender`).
    PortalGlow,
    /// No light map: the quad is tinted with the brightness of its block
    /// (`EntityFootStepFX` draws with `getLightBrightness`).
    BlockBrightness,
    /// The world light, with the sky component raised as the particle ages
    /// (`EntityEnchantmentTableParticleFX.getBrightnessForRender`, so the
    /// glyphs flare up near the table). See [`Particle::light`].
    AgeGlow,
    /// Always at the brightest light level: `getBrightnessForRender` returns
    /// a constant above the lightmap range (257 for a lava drip, 61680 for
    /// the large explosion).
    FullBright,
}

/// The top of the light map scale: a 4 bit light level times 16.
const MAX_LIGHT: i32 = 240;

/// What a particle shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Texture {
    /// Cell `index` of the 16 x 16 grid of `particles.png`
    /// (`setParticleTextureIndex`: column `index % 16`, row `index / 16`).
    Cell {
        /// The cell number, 0 to 255.
        index: u8,
    },
    /// The picture of a block in the block atlas (digging and dust); the
    /// client picks the sprite from the block's model. The sprite is sampled
    /// through the jitter window of [`Particle::jitter_u`] and
    /// [`Particle::jitter_v`].
    Block {
        /// The block the particle was torn from.
        state: umicraft_world::BlockState,
    },
    /// The particle picture of an item (`EntityBreakingFX`:
    /// `ItemModelMesher.getParticleIcon`), which the client finds in the atlas
    /// that holds the items. It is sampled through the jitter window like a
    /// block fragment.
    Item {
        /// The item id.
        id: u16,
        /// Its damage value (the metadata of items with subtypes).
        damage: u16,
    },
    /// A sprite named by its path (a picture beside `particles.png`).
    Sprite {
        /// The atlas path, e.g. `blocks/stone`.
        name: &'static str,
    },
}

/// The common state of a particle (MCP `EntityFX`).
#[derive(Debug, Clone)]
pub struct Particle {
    /// The position at the bottom centre of the box.
    pub position: DVec3,
    /// The position one tick ago, for the smooth draw.
    pub previous_position: DVec3,
    /// The velocity in blocks per tick.
    pub motion: DVec3,
    /// The age in ticks.
    pub age: u32,
    /// The age at which the particle dies.
    pub max_age: u32,
    /// The draw scale (the half side of the quad is `0.1 * scale`).
    pub scale: f32,
    /// The gravity factor (`0.04 * gravity` per tick).
    pub gravity: f32,
    /// The red, green and blue factors, 0 to 1.
    pub color: [f32; 3],
    /// The opacity, 0 to 1.
    pub alpha: f32,
    /// The picture and its layer.
    pub layer: Layer,
    /// What the particle samples.
    pub texture: Texture,
    /// The horizontal sub-cell offset of the block pictures, 0 to 3.
    pub jitter_u: f32,
    /// The vertical sub-cell offset of the block pictures, 0 to 3.
    pub jitter_v: f32,
    /// The side of the box (grown by [`Particle::scale_by`]).
    pub size: f64,
    /// Whether the last move ended on the ground.
    pub on_ground: bool,
    /// Whether the particle has died and can be dropped.
    pub dead: bool,
    /// Whether the particle ignores the blocks (`Entity.noClip`).
    pub no_clip: bool,
    /// How the particle is lit.
    pub lighting: Lighting,
    /// What the particle does each tick.
    pub behaviour: Behaviour,
}

impl Particle {
    /// A particle at rest, with the draws of the base constructor (MCP
    /// `EntityFX.java:49-64`): texture jitter, scale `(0.5 + r * 0.5) * 2` and
    /// a life of `4 / (r * 0.9 + 0.1)` ticks.
    #[must_use]
    pub fn new(position: DVec3, rng: &mut JavaRandom) -> Self {
        let jitter_u = rng.next_float() * 3.0;
        let jitter_v = rng.next_float() * 3.0;
        let scale = (rng.next_float() * 0.5 + 0.5) * 2.0;
        let max_age = (4.0 / (rng.next_float() * 0.9 + 0.1)) as u32;
        Self {
            position,
            previous_position: position,
            motion: DVec3::ZERO,
            age: 0,
            max_age,
            scale,
            gravity: 0.0,
            color: [1.0; 3],
            alpha: 1.0,
            layer: Layer::Misc,
            texture: Texture::Cell { index: 0 },
            jitter_u,
            jitter_v,
            size: BOX_SIZE,
            on_ground: false,
            dead: false,
            no_clip: false,
            lighting: Lighting::World,
            behaviour: Behaviour::Plain,
        }
    }

    /// A particle thrown with a velocity (MCP `EntityFX.java:66-79`): the
    /// speed is scattered by `+-0.4` per axis, then renormalised to a random
    /// length of `0.15 * (1 + r1 + r2) * 0.4` with a lift of `0.1`.
    #[must_use]
    pub fn with_velocity(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Self {
        let mut particle = Self::new(position, rng);
        let scattered = DVec3::new(
            speed.x + (rng.next_double() * 2.0 - 1.0) * VELOCITY_SCATTER,
            speed.y + (rng.next_double() * 2.0 - 1.0) * VELOCITY_SCATTER,
            speed.z + (rng.next_double() * 2.0 - 1.0) * VELOCITY_SCATTER,
        );
        let length = (rng.next_double() + rng.next_double() + 1.0) as f32 * 0.15;
        let norm = scattered.length() as f32;
        let unit = scattered / f64::from(norm);
        let motion = unit * f64::from(length) * VELOCITY_SCATTER;
        particle.motion = DVec3::new(motion.x, motion.y + VELOCITY_LIFT, motion.z);
        particle
    }

    /// `multiplyVelocity`: scales the motion, keeping the lift (MCP
    /// `EntityFX.java:81`).
    #[must_use]
    pub fn multiply_velocity(mut self, factor: f32) -> Self {
        let factor = f64::from(factor);
        self.motion.x *= factor;
        self.motion.y = (self.motion.y - VELOCITY_LIFT) * factor + VELOCITY_LIFT;
        self.motion.z *= factor;
        self
    }

    /// `multipleParticleScaleBy`: grows the quad and the box (MCP
    /// `EntityFX.java:89`).
    #[must_use]
    pub fn scale_by(mut self, factor: f32) -> Self {
        self.size = BOX_SIZE * f64::from(factor);
        self.scale *= factor;
        self
    }

    /// The box of the particle at the current position.
    #[must_use]
    pub fn bounding_box(&self) -> Aabb {
        let half = self.size / 2.0;
        Aabb::new(
            self.position + DVec3::new(-half, 0.0, -half),
            self.position + DVec3::new(half, self.size, half),
        )
    }

    /// The common update (MCP `EntityFX.onUpdate`, line 142): remember the
    /// position, age, fall, move with collision, drag and ground friction.
    ///
    /// The reference also runs the fluid, web and fire checks of
    /// `Entity.moveEntity`; they only change particles that sit in a web, so
    /// they are left out.
    pub fn tick<S: BlockSource + ?Sized>(&mut self, world: &S) {
        self.remember_position();
        self.grow_older();
        self.motion.y -= GRAVITY_STEP * f64::from(self.gravity);
        self.move_with_collision(world);
        self.drag_and_friction(AIR_DRAG);
    }

    /// `prevPos = pos`, the first line of every `onUpdate`.
    pub(super) fn remember_position(&mut self) {
        self.previous_position = self.position;
    }

    /// `if (particleAge++ >= particleMaxAge) setDead()`, in every `onUpdate`.
    pub(super) fn grow_older(&mut self) {
        self.age += 1;
        if self.age > self.max_age {
            self.dead = true;
        }
    }

    /// The end of most `onUpdate`s: the motion times `drag`, then the ground
    /// friction on X and Z.
    pub(super) fn drag_and_friction(&mut self, drag: f64) {
        self.motion *= drag;
        if self.on_ground {
            self.motion.x *= GROUND_FRICTION;
            self.motion.z *= GROUND_FRICTION;
        }
    }

    /// `Entity.moveEntity` for a box without step: moves, zeroes the motion on
    /// the axes that were blocked and sets `on_ground`. A `no_clip` particle
    /// only translates (the reference's `noClip` branch).
    pub(super) fn move_with_collision<S: BlockSource + ?Sized>(&mut self, world: &S) {
        if self.no_clip {
            self.position += self.motion;
            return;
        }
        let result = move_entity(
            self.bounding_box(),
            self.motion,
            world,
            MoveOptions::default(),
        );
        self.position += result.delta;
        if result.blocked_x {
            self.motion.x = 0.0;
        }
        if result.blocked_z {
            self.motion.z = 0.0;
        }
        if result.blocked_vertically {
            self.motion.y = 0.0;
        }
        self.on_ground = result.on_ground;
    }

    /// The light map coordinates to draw with, from the world light at the
    /// particle (`block` and `sky`, each 0 to 240) and the particle's own rule
    /// (MCP `getBrightnessForRender`); `partial_ticks` is the draw fraction of
    /// the tick. [`Lighting::BlockBrightness`] has no light map and returns the
    /// world light unchanged.
    #[must_use]
    pub fn light(&self, block: u8, sky: u8, partial_ticks: f32) -> (u8, u8) {
        let (block, sky) = (i32::from(block), i32::from(sky));
        let boost =
            |light: i32, progress: f32| (light + (progress * 15.0 * 16.0) as i32).min(MAX_LIGHT);
        let (block, sky) = match self.lighting {
            Lighting::World | Lighting::BlockBrightness => (block, sky),
            Lighting::FlameGlow => {
                let progress =
                    ((self.age as f32 + partial_ticks) / self.max_age as f32).clamp(0.0, 1.0);
                (boost(block, progress), sky)
            }
            // The glyphs of the enchantment table flare up near the table with
            // the fourth power of the age (`EntityEnchantmentTableParticleFX`).
            Lighting::AgeGlow => {
                let squared = (self.age as f32 / self.max_age as f32).powi(2);
                (block, boost(sky, squared * squared))
            }
            Lighting::LavaFull => (MAX_LIGHT, sky),
            Lighting::FullBright => (MAX_LIGHT, MAX_LIGHT),
            // The portal ignores the partial tick, MCP `EntityPortalFX.java:46-61`.
            Lighting::PortalGlow => {
                let squared = (self.age as f32 / self.max_age as f32).powi(2);
                let progress = squared * squared;
                (block, boost(sky, progress))
            }
        };
        (block as u8, sky as u8)
    }

    /// The position to draw at, `previous + (current - previous) * partial`.
    #[must_use]
    pub fn interpolated_position(&self, partial_ticks: f32) -> DVec3 {
        self.previous_position + (self.position - self.previous_position) * f64::from(partial_ticks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_world::World;

    fn rng() -> JavaRandom {
        JavaRandom::new(7)
    }

    #[test]
    fn constructor_draws_stay_in_the_reference_ranges() {
        let mut rng = rng();
        for _ in 0..200 {
            let particle = Particle::new(DVec3::ZERO, &mut rng);
            assert!((1.0..=2.0).contains(&particle.scale));
            assert!((4..=40).contains(&particle.max_age));
            assert!((0.0..3.0).contains(&particle.jitter_u));
            assert!((0.0..3.0).contains(&particle.jitter_v));
        }
    }

    #[test]
    fn thrown_particle_has_the_reference_speed_and_lift() {
        let mut rng = rng();
        for _ in 0..200 {
            let particle = Particle::with_velocity(DVec3::ZERO, DVec3::ZERO, &mut rng);
            let horizontal = DVec3::new(particle.motion.x, 0.0, particle.motion.z).length();
            // |unit| <= 1, length <= 0.45, so a horizontal speed <= 0.45 * 0.4.
            assert!(horizontal <= 0.45 * VELOCITY_SCATTER + 1e-9);
            assert!(particle.motion.y <= 0.45 * VELOCITY_SCATTER + VELOCITY_LIFT + 1e-9);
        }
    }

    #[test]
    fn multiply_velocity_keeps_the_lift() {
        let mut particle = Particle::new(DVec3::ZERO, &mut rng());
        particle.motion = DVec3::new(1.0, VELOCITY_LIFT, 1.0);
        let particle = particle.multiply_velocity(0.5);
        assert_eq!(particle.motion, DVec3::new(0.5, VELOCITY_LIFT, 0.5));
    }

    #[test]
    fn scale_by_grows_the_box_and_the_quad() {
        let mut particle = Particle::new(DVec3::ZERO, &mut rng());
        let before = particle.scale;
        particle = particle.scale_by(2.0);
        assert_eq!(particle.scale, before * 2.0);
        assert!((particle.size - 0.4).abs() < 1e-12);
    }

    #[test]
    fn particle_dies_after_its_life() {
        let world = World::new(0);
        let mut particle = Particle::new(DVec3::new(0.5, 100.0, 0.5), &mut rng());
        particle.max_age = 3;
        for _ in 0..3 {
            particle.tick(&world);
            assert!(!particle.dead);
        }
        particle.tick(&world);
        assert!(particle.dead);
    }

    #[test]
    fn gravity_accelerates_and_drag_slows() {
        let world = World::new(0);
        let mut particle = Particle::new(DVec3::new(0.5, 100.0, 0.5), &mut rng());
        particle.gravity = 1.0;
        particle.tick(&world);
        // motionY = (0 - 0.04) * 0.98 after the first tick.
        assert!((particle.motion.y - (-0.04 * AIR_DRAG)).abs() < 1e-12);
        assert!(particle.position.y < 100.0);
    }

    #[test]
    fn world_lighting_keeps_the_light_and_age_glow_raises_the_sky() {
        let mut particle = Particle::new(DVec3::ZERO, &mut rng());
        assert_eq!(particle.light(32, 64, 0.0), (32, 64));
        particle.lighting = Lighting::AgeGlow;
        particle.max_age = 10;
        particle.age = 0;
        assert_eq!(particle.light(32, 64, 0.0), (32, 64));
        // (5 / 10)^4 * 240 = 15.
        particle.age = 5;
        assert_eq!(particle.light(32, 64, 0.0), (32, 79));
        particle.age = 10;
        assert_eq!(particle.light(32, 64, 0.0), (32, 240));
    }

    #[test]
    fn interpolation_runs_from_previous_to_current() {
        let mut particle = Particle::new(DVec3::ZERO, &mut rng());
        particle.previous_position = DVec3::ZERO;
        particle.position = DVec3::new(2.0, 0.0, 0.0);
        assert_eq!(particle.interpolated_position(0.25).x, 0.5);
    }
}
