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
    _position: DVec3,
    _speed: DVec3,
    _rng: &mut JavaRandom,
) -> Option<Particle> {
    // Each family adds its kinds here as it is ported.
    let _ = kind;
    None
}
