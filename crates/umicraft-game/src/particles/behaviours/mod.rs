// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-type behaviour of the particles.
//!
//! Every family of `Entity*FX` classes gets its own module with a
//! constructor and an `update`; [`Behaviour`] is the closed set the engine
//! dispatches on, so a tick allocates nothing and needs no trait object.

use umicraft_core::java_random::JavaRandom;

mod flame;
mod footstep;
mod lava;
mod portal;
mod redstone;
mod smoke;

use super::kind::ParticleKind;
use super::particle::Particle;
use crate::shapes::BlockSource;
use glam::DVec3;

/// What a behaviour may read and do during a tick.
pub struct TickContext<'a, S: BlockSource + ?Sized> {
    /// The blocks, for collision and for the particles that look at them.
    pub world: &'a S,
    /// The random source of the engine.
    pub rng: &'a mut JavaRandom,
    /// Particles created by this tick (a drop that splashes, an explosion
    /// that scatters smoke); the engine adds them after the tick.
    pub spawned: &'a mut Vec<Particle>,
}

/// What a particle does each tick beyond being an `EntityFX`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Behaviour {
    /// Only the common update of `EntityFX` (the `Barrier` particle and the
    /// base class itself).
    Plain,
    /// `EntityFlameFX`; `base_scale` is the scale it shrinks from.
    Flame {
        /// The scale at birth (`flameScale`).
        base_scale: f32,
    },
    /// `EntitySmokeFX` and `EntityCritFX` (large smoke).
    Smoke {
        /// The scale it grows to (`smokeParticleScale`).
        base_scale: f32,
    },
    /// `EntityLavaFX`.
    Lava {
        /// The scale it shrinks from (`lavaParticleScale`).
        base_scale: f32,
    },
    /// `EntityPortalFX`.
    Portal {
        /// Where it was created (`portalPosX/Y/Z`), the end of its curve.
        origin: DVec3,
        /// The scale it grows to (`portalParticleScale`).
        base_scale: f32,
    },
    /// `EntityReddustFX`.
    Redstone {
        /// The scale it grows to (`reddustParticleScale`).
        base_scale: f32,
    },
    /// `EntityFootStepFX`, drawn flat on the ground (see [`footstep`]).
    Footstep,
}

impl Behaviour {
    /// Runs one tick of `particle`; the behaviour decides whether it calls the
    /// common [`Particle::tick`] (most `onUpdate` overrides do not).
    pub(super) fn update<S: BlockSource + ?Sized>(
        self,
        particle: &mut Particle,
        context: &mut TickContext<'_, S>,
    ) {
        match self {
            Self::Plain => particle.tick(context.world),
            Self::Flame { .. } => flame::update(particle, context),
            Self::Smoke { .. } => smoke::update(particle, context),
            Self::Lava { .. } => lava::update(particle, context),
            Self::Portal { origin, .. } => portal::update(particle, origin),
            Self::Redstone { .. } => redstone::update(particle, context),
            Self::Footstep => footstep::update(particle),
        }
    }

    /// The scale to draw `particle` with. The reference recomputes
    /// `particleScale` from the age inside `renderParticle`; here the stored
    /// `scale` stays the constructor's and the renderer asks for this value.
    #[must_use]
    pub fn render_scale(self, particle: &Particle, partial_ticks: f32) -> f32 {
        match self {
            Self::Flame { base_scale } => flame::render_scale(base_scale, particle, partial_ticks),
            Self::Smoke { base_scale } | Self::Redstone { base_scale } => {
                smoke::render_scale(base_scale, particle, partial_ticks)
            }
            Self::Lava { base_scale } => lava::render_scale(base_scale, particle, partial_ticks),
            Self::Portal { base_scale, .. } => {
                portal::render_scale(base_scale, particle, partial_ticks)
            }
            _ => particle.scale,
        }
    }

    /// The opacity to draw `particle` with: the footprint fades out, the
    /// others use their `alpha`.
    #[must_use]
    pub fn render_alpha(self, particle: &Particle, partial_ticks: f32) -> f32 {
        match self {
            Self::Footstep => footstep::alpha(particle, partial_ticks),
            _ => particle.alpha,
        }
    }
}

/// Builds the particle a `Particle` packet or a game event asks for: the
/// `IParticleFactory.getEntityFX` of `kind` (MCP `EffectRenderer`,
/// `registerVanillaParticles`).
///
/// `speed` is the packet's velocity triple. Returns `None` for the kinds whose
/// family is not ported yet and for the ones that need data this crate does
/// not have (the block and item crack kinds are built by the client, which
/// knows the sprites).
#[must_use]
pub fn create(
    kind: ParticleKind,
    position: DVec3,
    speed: DVec3,
    rng: &mut JavaRandom,
) -> Option<Particle> {
    // Each family adds its kinds here as it is ported.
    Some(match kind {
        ParticleKind::Flame => flame::create(position, speed, rng),
        ParticleKind::Smoke => smoke::create(position, speed, smoke::SMOKE_FACTOR, rng),
        ParticleKind::LargeSmoke => smoke::create(position, speed, smoke::LARGE_SMOKE_FACTOR, rng),
        ParticleKind::Lava => lava::create(position, rng),
        ParticleKind::Portal => portal::create(position, speed, rng),
        // The packet's speed is the dust colour, narrowed to floats.
        ParticleKind::Redstone => redstone::create(
            position,
            [speed.x as f32, speed.y as f32, speed.z as f32],
            rng,
        ),
        ParticleKind::Footstep => footstep::create(position, rng),
        _ => return None,
    })
}

/// `(int)(base / (Math.random() * 0.8 + 0.2))`, the life draw shared by most
/// particle constructors (a life of `base` to `5 * base` ticks).
fn random_life(base: f64, rng: &mut JavaRandom) -> u32 {
    (base / (rng.next_double() * 0.8 + 0.2)) as u32
}
