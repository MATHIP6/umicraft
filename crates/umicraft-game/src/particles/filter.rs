// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-category particle switches of the video options (OptiFine's
//! Animations page), checked before a particle is built.
//!
//! OptiFine filters in `RenderGlobal.spawnEffectParticle` by particle id and
//! in `EffectRenderer.addEffect` for firework sparks; the rain splash switch
//! gates `EntityRenderer.addRainParticles`. Here the categories are a `Copy`
//! bitset so the check allocates nothing and the whole value crosses to the
//! game thread in one event.

use super::behaviours::Behaviour;
use super::kind::ParticleKind;

/// One switch of the Animations page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ParticleCategory {
    /// `reddust`.
    Redstone,
    /// `explode`, `largeexplode`, `hugeexplosion`.
    Explosion,
    /// `flame`.
    Flame,
    /// `smoke`, `largesmoke`.
    Smoke,
    /// `depthsuspend`, the void fog.
    Void,
    /// `suspended`. OptiFine leaves bubbles, splashes and wakes alone.
    Water,
    /// The ground splashes of rain (`addRainParticles` as a whole).
    RainSplash,
    /// `portal`.
    Portal,
    /// `spell`, `instantSpell`, `mobSpell`, `mobSpellAmbient`, `witchMagic`.
    Potion,
    /// `dripWater`, `dripLava`.
    Dripping,
    /// `fireworksSpark` and the sparks of a firework explosion.
    Firework,
}

impl ParticleCategory {
    /// The switch that governs `kind`, or `None` when it has none.
    #[must_use]
    pub fn of(kind: ParticleKind) -> Option<Self> {
        use ParticleKind as K;
        Some(match kind {
            K::Redstone => Self::Redstone,
            K::Explode | K::LargeExplode | K::HugeExplosion => Self::Explosion,
            K::Flame => Self::Flame,
            K::Smoke | K::LargeSmoke => Self::Smoke,
            K::DepthSuspend => Self::Void,
            K::Suspended => Self::Water,
            K::Portal => Self::Portal,
            K::Spell | K::InstantSpell | K::MobSpell | K::MobSpellAmbient | K::WitchMagic => {
                Self::Potion
            }
            K::DripWater | K::DripLava => Self::Dripping,
            K::FireworksSpark => Self::Firework,
            _ => return None,
        })
    }

    const fn bit(self) -> u16 {
        1 << self as u8
    }
}

/// Which categories may create particles. The default allows all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParticleFilter {
    enabled: u16,
}

impl Default for ParticleFilter {
    fn default() -> Self {
        Self { enabled: u16::MAX }
    }
}

impl ParticleFilter {
    /// This filter with `category` switched `on` or off.
    #[must_use]
    pub fn with(mut self, category: ParticleCategory, on: bool) -> Self {
        if on {
            self.enabled |= category.bit();
        } else {
            self.enabled &= !category.bit();
        }
        self
    }

    /// Whether `category` creates particles.
    #[must_use]
    pub fn allows(self, category: ParticleCategory) -> bool {
        self.enabled & category.bit() != 0
    }

    /// Whether a particle of `kind` may be created.
    #[must_use]
    pub fn allows_kind(self, kind: ParticleKind) -> bool {
        ParticleCategory::of(kind).is_none_or(|category| self.allows(category))
    }

    /// Whether a particle with this behaviour may be added: only the firework
    /// sparks are filtered there (`EffectRenderer.addEffect`).
    #[must_use]
    pub(super) fn allows_behaviour(self, behaviour: Behaviour) -> bool {
        !matches!(behaviour, Behaviour::Spark(_)) || self.allows(ParticleCategory::Firework)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILTERED: [(ParticleKind, ParticleCategory); 17] = [
        (ParticleKind::Redstone, ParticleCategory::Redstone),
        (ParticleKind::Explode, ParticleCategory::Explosion),
        (ParticleKind::LargeExplode, ParticleCategory::Explosion),
        (ParticleKind::HugeExplosion, ParticleCategory::Explosion),
        (ParticleKind::Flame, ParticleCategory::Flame),
        (ParticleKind::Smoke, ParticleCategory::Smoke),
        (ParticleKind::LargeSmoke, ParticleCategory::Smoke),
        (ParticleKind::DepthSuspend, ParticleCategory::Void),
        (ParticleKind::Suspended, ParticleCategory::Water),
        (ParticleKind::Portal, ParticleCategory::Portal),
        (ParticleKind::Spell, ParticleCategory::Potion),
        (ParticleKind::InstantSpell, ParticleCategory::Potion),
        (ParticleKind::MobSpell, ParticleCategory::Potion),
        (ParticleKind::MobSpellAmbient, ParticleCategory::Potion),
        (ParticleKind::WitchMagic, ParticleCategory::Potion),
        (ParticleKind::DripWater, ParticleCategory::Dripping),
        (ParticleKind::FireworksSpark, ParticleCategory::Firework),
    ];

    #[test]
    fn each_kind_is_rejected_only_when_its_category_is_off() {
        for (kind, category) in FILTERED {
            assert!(ParticleFilter::default().allows_kind(kind), "{kind:?}");
            let off = ParticleFilter::default().with(category, false);
            assert!(!off.allows_kind(kind), "{kind:?}");
            assert!(off.with(category, true).allows_kind(kind), "{kind:?}");
        }
    }

    #[test]
    fn switching_one_category_off_keeps_the_others() {
        let off = ParticleFilter::default().with(ParticleCategory::Smoke, false);
        assert!(off.allows_kind(ParticleKind::Flame));
        assert!(off.allows(ParticleCategory::RainSplash));
    }

    #[test]
    fn kinds_without_a_switch_are_always_allowed() {
        let all_off = [ParticleCategory::Smoke, ParticleCategory::Water]
            .into_iter()
            .fold(ParticleFilter::default(), |filter, c| filter.with(c, false));
        assert!(all_off.allows_kind(ParticleKind::Bubble));
        assert!(all_off.allows_kind(ParticleKind::Crit));
    }
}
