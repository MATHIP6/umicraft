// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-type behaviour of the particles.
//!
//! Every family of `Entity*FX` classes gets its own module with a
//! constructor and an `update`; [`Behaviour`] is the closed set the engine
//! dispatches on, so a tick allocates nothing and needs no trait object.

use umicraft_core::java_random::JavaRandom;

use super::kind::ParticleKind;
use super::particle::Particle;
use crate::shapes::BlockSource;
use glam::DVec3;
use umicraft_world::fluid::Liquid;

mod drip;
mod explode;
mod rain;
mod shared;
mod underwater;

pub use explode::{
    large_explode_quad, LargeExplodeQuad, LARGE_EXPLODE_FRAME_SPAN, LARGE_EXPLODE_TEXTURE,
};

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
    /// `EntityBubbleFX`: rises in water, dies out of it.
    Bubble,
    /// `EntitySuspendFX`: a still speck that dies out of water.
    Suspended,
    /// `EntityFishWakeFX`: a ripple that grows and cycles its picture.
    Wake,
    /// `EntityRainFX` and `EntitySplashFX`: a droplet that dies on the
    /// ground or under a surface.
    Rain,
    /// `EntityDropParticleFX`: a drop of `liquid` that hangs for `bob_timer`
    /// more ticks, then falls.
    Drip {
        /// The liquid that drips.
        liquid: Liquid,
        /// The ticks left before the drop falls; negative once it falls.
        bob_timer: i32,
    },
    /// `EntityExplodeFX`: a smoke puff that fades through eight pictures.
    Explode,
    /// `EntityLargeExplodeFX`: the animated flash; `size` is the packet's
    /// `1 - speed.x / 2`.
    LargeExplode {
        /// The factor of the quad size (`field_70582_as`).
        size: f32,
    },
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
            Self::Bubble => underwater::update_bubble(particle, context),
            Self::Suspended => underwater::update_suspended(particle, context),
            Self::Wake => underwater::update_wake(particle, context),
            Self::Rain => rain::update(particle, context),
            Self::Drip { liquid, bob_timer } => drip::update(liquid, bob_timer, particle, context),
            Self::Explode => explode::update_puff(particle, context),
            Self::LargeExplode { .. } => explode::update_flash(particle),
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
    match kind {
        ParticleKind::Bubble => Some(underwater::bubble(position, speed, rng)),
        ParticleKind::Suspended => Some(underwater::suspended(position, speed, rng)),
        ParticleKind::Wake => Some(underwater::wake(position, speed, rng)),
        ParticleKind::Droplet => Some(rain::droplet(position, rng)),
        ParticleKind::Splash => Some(rain::splash(position, speed, rng)),
        ParticleKind::DripWater => Some(drip::drop(position, Liquid::Water, rng)),
        ParticleKind::DripLava => Some(drip::drop(position, Liquid::Lava, rng)),
        ParticleKind::Explode => Some(explode::puff(position, speed, rng)),
        ParticleKind::LargeExplode => Some(explode::large_flash(position, speed, rng)),
        _ => None,
    }
}
