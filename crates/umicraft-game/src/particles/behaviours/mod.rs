// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-type behaviour of the particles.
//!
//! Every family of `Entity*FX` classes gets its own module with a
//! constructor and an `update`; [`Behaviour`] is the closed set the engine
//! dispatches on, so a tick allocates nothing and needs no trait object.

mod aura;
mod breaking;
mod cloud;
mod common;
mod crit;
mod enchantment;
mod firework;
mod heart;
mod huge_explode;
mod note;
mod snow_shovel;
mod spell;

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
use umicraft_world::fluid::Liquid;

pub(super) mod digging;
mod drip;
mod explode;
mod rain;
mod shared;
mod underwater;

pub use explode::{
    large_explode_quad, LargeExplodeQuad, LARGE_EXPLODE_FRAME_SPAN, LARGE_EXPLODE_TEXTURE,
};
pub use firework::{spark as firework_spark, SparkStyle};

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
    /// `EntityHugeExplodeFX`: an invisible emitter of large explosions.
    HugeExplosion {
        /// The ticks it has emitted (`timeSinceStart`).
        ticks: u32,
    },
    /// `EntityFirework.SparkFX`: a firework spark.
    Spark(SparkStyle),
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
            Self::Spell { base_cell } => spell::update(particle, base_cell, context),
            Self::Crit => crit::update(particle, context),
            Self::Heart => heart::update(particle, context),
            Self::Aura => aura::update(particle, context),
            Self::Note => note::update(particle, context),
            Self::Enchantment { table } => enchantment::update(particle, table, context),
            Self::Cloud => cloud::update(particle, context),
            Self::SnowShovel => snow_shovel::update(particle, context),
            Self::Bubble => underwater::update_bubble(particle, context),
            Self::Suspended => underwater::update_suspended(particle, context),
            Self::Wake => underwater::update_wake(particle, context),
            Self::Rain => rain::update(particle, context),
            Self::Drip { liquid, bob_timer } => drip::update(liquid, bob_timer, particle, context),
            Self::Explode => explode::update_puff(particle, context),
            Self::LargeExplode { .. } => explode::update_flash(particle),
            Self::HugeExplosion { ticks } => huge_explode::update(particle, ticks, context),
            Self::Spark(style) => firework::update(particle, style, context),
        }
    }

    /// The scale to draw `particle` with at `partial_ticks` into the tick.
    ///
    /// The reference recomputes `particleScale` inside `renderParticle`; the
    /// stored `scale` stays the constructor's and the renderer asks for this
    /// value. Flame, smoke, redstone, lava and portal follow their own curves;
    /// crit, heart, note, cloud and snow shovel grow the quad from nothing over
    /// the first 1/32 of the life (`scale * clamp((age + partial) / life * 32,
    /// 0, 1)`); the others draw `particle.scale`.
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
            Self::Crit | Self::Heart | Self::Note | Self::Cloud | Self::SnowShovel => {
                particle.scale * common::growth(particle, partial_ticks)
            }
            Self::HugeExplosion { .. } => 0.0,
            Self::Spark(style) => firework::render_scale(style, particle),
            _ => particle.scale,
        }
    }

    /// The opacity to draw `particle` with: the footprint fades out, the
    /// others use their `alpha`.
    #[must_use]
    pub fn render_alpha(self, particle: &Particle, partial_ticks: f32) -> f32 {
        match self {
            Self::Footstep => footstep::alpha(particle, partial_ticks),
            Self::HugeExplosion { .. } => 0.0,
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
    use ParticleKind as K;
    // Each family adds its kinds here as it is ported.
    match kind {
        K::Flame => Some(flame::create(position, speed, rng)),
        K::Smoke => Some(smoke::create(position, speed, smoke::SMOKE_FACTOR, rng)),
        K::LargeSmoke => Some(smoke::create(
            position,
            speed,
            smoke::LARGE_SMOKE_FACTOR,
            rng,
        )),
        K::Lava => Some(lava::create(position, rng)),
        K::Portal => Some(portal::create(position, speed, rng)),
        // The packet's speed is the dust colour, narrowed to floats.
        K::Redstone => Some(redstone::create(
            position,
            [speed.x as f32, speed.y as f32, speed.z as f32],
            rng,
        )),
        K::Footstep => Some(footstep::create(position, rng)),
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
        K::Slime => Some(breaking::slime(position, rng)),
        K::Bubble => Some(underwater::bubble(position, speed, rng)),
        K::Suspended => Some(underwater::suspended(position, speed, rng)),
        K::Wake => Some(underwater::wake(position, speed, rng)),
        K::Droplet => Some(rain::droplet(position, rng)),
        K::Splash => Some(rain::splash(position, speed, rng)),
        K::DripWater => Some(drip::drop(position, Liquid::Water, rng)),
        K::DripLava => Some(drip::drop(position, Liquid::Lava, rng)),
        K::Explode => Some(explode::puff(position, speed, rng)),
        K::LargeExplode => Some(explode::large_flash(position, speed, rng)),
        K::HugeExplosion => Some(huge_explode::create(position, rng)),
        K::FireworksSpark => Some(firework::spark(position, speed, SparkStyle::default(), rng)),
        _ => None,
    }
}

/// `(int)(base / (Math.random() * 0.8 + 0.2))`, the life draw shared by most
/// particle constructors (a life of `base` to `5 * base` ticks).
fn random_life(base: f64, rng: &mut JavaRandom) -> u32 {
    (base / (rng.next_double() * 0.8 + 0.2)) as u32
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
