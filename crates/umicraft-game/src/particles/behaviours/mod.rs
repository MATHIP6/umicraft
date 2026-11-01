// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-type behaviour of the particles.
//!
//! Every family of `Entity*FX` classes gets its own module with a
//! constructor and an `update`; [`Behaviour`] is the closed set the engine
//! dispatches on, so a tick allocates nothing and needs no trait object.

mod aura;
mod cloud;
mod common;
mod crit;
mod enchantment;
mod heart;
mod note;
mod snow_shovel;
mod spell;

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
    /// `EntitySpellParticleFX`: potion sparkles; `base_cell` is the first of
    /// the eight pictures it steps through (`baseSpellTextureIndex`).
    Spell {
        /// The first cell of the animation, 128 or 144.
        base_cell: u8,
    },
    /// `EntityCrit2FX`: the critical hit star.
    Crit,
    /// `EntityHeartFX`: the heart and the angry villager.
    Heart,
    /// `EntityAuraFX`: the drifting speck (mycelium, deep water, happy
    /// villager).
    Aura,
    /// `EntityNoteFX`: the note block note.
    Note,
    /// `EntityEnchantmentTableParticleFX`: a glyph flying to the table.
    Enchantment {
        /// The table the glyph flies to (`coordX`, `coordY`, `coordZ`).
        table: DVec3,
    },
    /// `EntityCloudFX`: the white puff.
    Cloud,
    /// `EntitySnowShovelFX`: the thrown snow.
    SnowShovel,
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
            Self::Spell { base_cell } => spell::update(particle, base_cell, context),
            Self::Crit => crit::update(particle, context),
            Self::Heart => heart::update(particle, context),
            Self::Aura => aura::update(particle, context),
            Self::Note => note::update(particle, context),
            Self::Enchantment { table } => enchantment::update(particle, table, context),
            Self::Cloud => cloud::update(particle, context),
            Self::SnowShovel => snow_shovel::update(particle, context),
        }
    }

    /// The scale to draw `particle` with at `partial_ticks` into the tick.
    ///
    /// The crit, heart, note, cloud and snow shovel overrides of
    /// `renderParticle` grow the quad from nothing to its size over the first
    /// 1/32 of the life (`scale * clamp((age + partial) / life * 32, 0, 1)`);
    /// the other families draw `particle.scale`. Colour and opacity are never
    /// overridden by the families ported so far, so they are read from the
    /// particle as is.
    #[must_use]
    pub fn render_scale(self, particle: &Particle, partial_ticks: f32) -> f32 {
        match self {
            Self::Crit | Self::Heart | Self::Note | Self::Cloud | Self::SnowShovel => {
                particle.scale * common::growth(particle, partial_ticks)
            }
            _ => particle.scale,
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
    use ParticleKind as K;
    // Each family adds its kinds here as it is ported.
    match kind {
        K::Spell | K::InstantSpell | K::MobSpell | K::MobSpellAmbient | K::WitchMagic => {
            Some(spell::create(kind, position, speed, rng))
        }
        K::Crit | K::MagicCrit => Some(crit::create(kind, position, speed, rng)),
        K::Heart | K::AngryVillager => Some(heart::create(kind, position, speed, rng)),
        K::TownAura | K::HappyVillager | K::DepthSuspend => {
            Some(aura::create(kind, position, speed, rng))
        }
        K::Note => Some(note::create(position, speed, rng)),
        K::EnchantmentTable => Some(enchantment::create(position, speed, rng)),
        K::Cloud => Some(cloud::create(position, speed, rng)),
        K::SnowShovel => Some(snow_shovel::create(position, speed, rng)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORTED: [ParticleKind; 16] = {
        use ParticleKind::*;
        [
            Spell,
            InstantSpell,
            MobSpell,
            MobSpellAmbient,
            WitchMagic,
            Crit,
            MagicCrit,
            Heart,
            AngryVillager,
            TownAura,
            HappyVillager,
            DepthSuspend,
            Note,
            EnchantmentTable,
            Cloud,
            SnowShovel,
        ]
    };

    #[test]
    fn the_factory_builds_every_ported_kind() {
        let mut rng = JavaRandom::new(1);
        for kind in PORTED {
            assert!(create(kind, DVec3::ZERO, DVec3::ZERO, &mut rng).is_some());
        }
    }

    #[test]
    fn the_growth_overrides_scale_the_quad_up_over_the_first_thirty_second() {
        let mut rng = JavaRandom::new(1);
        let mut puff = create(ParticleKind::Cloud, DVec3::ZERO, DVec3::ZERO, &mut rng)
            .expect("cloud is ported");
        puff.age = 0;
        puff.max_age = 64;
        assert_eq!(puff.behaviour.render_scale(&puff, 0.0), 0.0);
        // (0 + 1) / 64 * 32 = 0.5.
        assert_eq!(puff.behaviour.render_scale(&puff, 1.0), puff.scale * 0.5);
        puff.age = 10;
        assert_eq!(puff.behaviour.render_scale(&puff, 0.0), puff.scale);
    }

    #[test]
    fn the_other_families_draw_their_own_scale() {
        let mut rng = JavaRandom::new(1);
        let spark = create(ParticleKind::Spell, DVec3::ZERO, DVec3::ZERO, &mut rng)
            .expect("spell is ported");
        assert_eq!(spark.behaviour.render_scale(&spark, 0.5), spark.scale);
    }
}
