// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particle types of the 1.8 protocol.

/// A particle type, numbered like `EnumParticleTypes` (MCP
/// `net/minecraft/util/EnumParticleTypes.java:10-51`).
///
/// The ids are the wire ids of the `Particle` packet, so the order must not
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ParticleKind {
    /// `explode`
    Explode = 0,
    /// `largeexplode`
    LargeExplode,
    /// `hugeexplosion`
    HugeExplosion,
    /// `fireworksSpark`
    FireworksSpark,
    /// `bubble`
    Bubble,
    /// `splash`
    Splash,
    /// `wake`
    Wake,
    /// `suspended`
    Suspended,
    /// `depthsuspend`
    DepthSuspend,
    /// `crit`
    Crit,
    /// `magicCrit`
    MagicCrit,
    /// `smoke`
    Smoke,
    /// `largesmoke`
    LargeSmoke,
    /// `spell`
    Spell,
    /// `instantSpell`
    InstantSpell,
    /// `mobSpell`
    MobSpell,
    /// `mobSpellAmbient`
    MobSpellAmbient,
    /// `witchMagic`
    WitchMagic,
    /// `dripWater`
    DripWater,
    /// `dripLava`
    DripLava,
    /// `angryVillager`
    AngryVillager,
    /// `happyVillager`
    HappyVillager,
    /// `townaura`
    TownAura,
    /// `note`
    Note,
    /// `portal`
    Portal,
    /// `enchantmenttable`
    EnchantmentTable,
    /// `flame`
    Flame,
    /// `lava`
    Lava,
    /// `footstep`
    Footstep,
    /// `cloud`
    Cloud,
    /// `reddust`
    Redstone,
    /// `snowballpoof`
    Snowball,
    /// `snowshovel`
    SnowShovel,
    /// `slime`
    Slime,
    /// `heart`
    Heart,
    /// `barrier`
    Barrier,
    /// `iconcrack_` (arguments: item id, damage)
    ItemCrack,
    /// `blockcrack_` (argument: block state id)
    BlockCrack,
    /// `blockdust_` (argument: block state id)
    BlockDust,
    /// `droplet`
    Droplet,
    /// `take`
    ItemTake,
    /// `mobappearance`
    MobAppearance,
}

/// Every kind, indexed by wire id.
const ALL: [ParticleKind; 42] = {
    use ParticleKind::*;
    [
        Explode,
        LargeExplode,
        HugeExplosion,
        FireworksSpark,
        Bubble,
        Splash,
        Wake,
        Suspended,
        DepthSuspend,
        Crit,
        MagicCrit,
        Smoke,
        LargeSmoke,
        Spell,
        InstantSpell,
        MobSpell,
        MobSpellAmbient,
        WitchMagic,
        DripWater,
        DripLava,
        AngryVillager,
        HappyVillager,
        TownAura,
        Note,
        Portal,
        EnchantmentTable,
        Flame,
        Lava,
        Footstep,
        Cloud,
        Redstone,
        Snowball,
        SnowShovel,
        Slime,
        Heart,
        Barrier,
        ItemCrack,
        BlockCrack,
        BlockDust,
        Droplet,
        ItemTake,
        MobAppearance,
    ]
};

impl ParticleKind {
    /// The kind with the given wire id, or `None` outside `0..=41`.
    #[must_use]
    pub fn from_id(id: i32) -> Option<Self> {
        usize::try_from(id)
            .ok()
            .and_then(|index| ALL.get(index))
            .copied()
    }

    /// The wire id of this kind.
    #[must_use]
    pub fn id(self) -> i32 {
        i32::from(self as u8)
    }

    /// Whether the particle is shown from far away without the long-distance
    /// flag (`EnumParticleTypes.func_179344_e`, the third constructor
    /// argument: explosions and the guardian apparition).
    #[must_use]
    pub fn always_shown(self) -> bool {
        matches!(
            self,
            Self::Explode | Self::LargeExplode | Self::HugeExplosion | Self::MobAppearance
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_for_every_kind() {
        for id in 0..42 {
            let kind = ParticleKind::from_id(id).expect("known id");
            assert_eq!(kind.id(), id);
        }
    }

    #[test]
    fn ids_outside_the_table_are_unknown() {
        assert_eq!(ParticleKind::from_id(-1), None);
        assert_eq!(ParticleKind::from_id(42), None);
    }

    #[test]
    fn named_ids_match_the_reference() {
        assert_eq!(ParticleKind::Flame.id(), 26);
        assert_eq!(ParticleKind::Barrier.id(), 35);
        assert_eq!(ParticleKind::BlockCrack.id(), 37);
        assert_eq!(ParticleKind::MobAppearance.id(), 41);
    }

    #[test]
    fn only_the_reference_kinds_are_always_shown() {
        let shown: Vec<i32> = (0..42)
            .filter(|&id| ParticleKind::from_id(id).is_some_and(ParticleKind::always_shown))
            .collect();
        assert_eq!(shown, [0, 1, 2, 41]);
    }
}
