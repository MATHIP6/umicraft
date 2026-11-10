// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client-side particles (P8): the simulation of `EffectRenderer` and the
//! `Entity*FX` classes of the 1.8 reference, without any drawing.
//!
//! | Module | Owns |
//! |---|---|
//! | `kind` | [`ParticleKind`], the 42 wire ids |
//! | `particle` | [`Particle`], the shared `EntityFX` state and tick |
//! | `behaviours` | [`Behaviour`], one module per family of particle classes |
//! | `ambient` | the particles blocks, rain and sprinting make by themselves |
//! | `block_effects` | the fragments of a broken or hit block |
//! | `effect` | the particles of `playAuxSFX` (`Effect` packet) |
//! | `explosion` | the particles of an `Explosion` packet |
//! | `mobs` | the particles mobs give off by themselves (smoke, sparks, hearts) |
//! | `spawn` | [`ParticleSetting`], the range filter and the `Particle` packet |
//! | `engine` | [`ParticleEngine`], the buckets, the 4000 cap and the tick |
//!
//! The game thread ticks the engine at 20 Hz; the renderer only reads the
//! particles it hands out, so the render thread never touches the world (as
//! for the weather, ADR-0031). The plan is in `docs/tasks/p8/README.md`.

pub mod ambient;
pub mod behaviours;
mod block_effects;
mod effect;
mod engine;
mod explosion;
mod kind;
mod mobs;
mod particle;
mod spawn;

pub use behaviours::{create, firework_spark, Behaviour, SparkStyle, TickContext};
pub use effect::EffectContext;
pub use engine::{ParticleEngine, MAX_PER_BUCKET};
pub use kind::ParticleKind;
pub use mobs::MobBody;
pub use particle::{Layer, Lighting, Particle, Texture};
pub use spawn::{ParticleSetting, SpawnFilter, SpawnRequest};
