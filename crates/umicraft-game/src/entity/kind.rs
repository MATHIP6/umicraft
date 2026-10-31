// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity identifiers and the closed set of entity categories.

/// Unique identifier of an entity in the world (32-bit signed network ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub i32);

/// The category of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    /// Remote player.
    Player,
    /// Zombie.
    Zombie,
    /// Skeleton.
    Skeleton,
    /// Creeper.
    Creeper,
    /// Cow.
    Cow,
    /// Pig.
    Pig,
    /// Sheep.
    Sheep,
    /// Chicken.
    Chicken,
    /// Spider.
    Spider,
    /// Cave Spider.
    CaveSpider,
    /// Enderman.
    Enderman,
    /// Slime.
    Slime,
    /// Magma Cube.
    MagmaCube,
    /// Zombie Pigman.
    ZombiePigman,
    /// Giant Zombie.
    Giant,
    /// Villager.
    Villager,
    /// Zombie Villager.
    ZombieVillager,
    /// Witch.
    Witch,
    /// Iron Golem.
    IronGolem,
    /// Snow Golem / Snowman.
    Snowman,
    /// Wolf / Dog.
    Wolf,
    /// Mooshroom (Mushroom Cow).
    Mooshroom,
    /// Squid.
    Squid,
    /// Bat.
    Bat,
    /// Blaze.
    Blaze,
    /// Ender Dragon.
    Dragon,
    /// Endermite.
    Endermite,
    /// Ghast.
    Ghast,
    /// Guardian.
    Guardian,
    /// Horse / Donkey / Mule.
    Horse,
    /// Ocelot / Cat.
    Ocelot,
    /// Rabbit.
    Rabbit,
    /// Silverfish.
    Silverfish,
    /// Wither.
    Wither,
    /// Dropped item stack.
    Item,
    /// Projectile arrow.
    Arrow,
    /// Item frame hanging on a wall.
    ItemFrame,
    /// Armour stand (object type 78).
    ArmorStand,
    /// The bobber of a fishing rod (object type 90); its `data` is the
    /// angler entity id.
    FishHook,
    /// A boat (object type 1).
    Boat,
    /// A minecart (object type 10); its `data` is the cart type.
    Minecart,
    /// Primed TNT (object type 50).
    PrimedTnt,
    /// An ender crystal (object type 51).
    EnderCrystal,
    /// A snowball (object type 61).
    Snowball,
    /// A thrown egg (object type 62).
    Egg,
    /// A large fireball of a ghast (object type 63).
    LargeFireball,
    /// A small fireball of a blaze (object type 64).
    SmallFireball,
    /// An ender pearl (object type 65).
    EnderPearl,
    /// A wither skull (object type 66).
    WitherSkull,
    /// A falling block (object type 70); its `data` is the block state.
    FallingBlock,
    /// An eye of ender (object type 72).
    EyeOfEnder,
    /// A thrown potion (object type 73); its `data` is the potion damage.
    Potion,
    /// A bottle o enchanting (object type 75).
    ExpBottle,
    /// A firework rocket (object type 76).
    FireworkRocket,
    /// The knot of a leash on a fence (object type 77).
    LeashKnot,
    /// An experience orb (`Spawn Experience Orb`); its `object_data` is the
    /// experience it is worth.
    XpOrb,
    /// A painting (`Spawn Painting`); its `object_data` is the index of its
    /// artwork in [`super::painting::ARTS`].
    Painting,
    /// A lightning bolt (`Spawn Global Entity`); its `object_data` seeds the
    /// shape of the bolt.
    Lightning,
    /// Other mob or entity type by raw numeric identifier.
    Other(u8),
}

impl EntityKind {
    /// The height of the hitbox in blocks (MCP `Entity.height`), which puts
    /// the name tag above the head. Unlisted kinds count as player-high.
    #[must_use]
    pub const fn height(self) -> f32 {
        match self {
            Self::Zombie | Self::ZombiePigman => 1.95,
            Self::Skeleton => 1.99,
            Self::Creeper => 1.7,
            Self::Cow | Self::Mooshroom => 1.4,
            Self::Pig | Self::Spider => 0.9,
            Self::Sheep => 1.3,
            Self::Chicken | Self::Ocelot | Self::Rabbit | Self::Silverfish => 0.7,
            Self::CaveSpider => 0.5,
            Self::Enderman | Self::IronGolem => 2.9,
            Self::Wolf => 0.8,
            Self::Horse => 1.6,
            Self::Snowman => 1.9,
            Self::Giant => 10.8,
            Self::Ghast => 4.0,
            Self::Guardian => 0.85,
            Self::Endermite => 0.3,
            Self::Wither => 3.5,
            Self::Arrow => 0.5,
            Self::Item => 0.25,
            _ => 1.8,
        }
    }

    /// The width of the hitbox in blocks (MCP `Entity.width`): the side of
    /// its square footprint. Unlisted kinds count as player-wide.
    #[must_use]
    pub const fn width(self) -> f32 {
        match self {
            Self::Spider | Self::Horse | Self::IronGolem => 1.4,
            Self::CaveSpider | Self::Snowman => 0.7,
            Self::Cow | Self::Pig | Self::Sheep | Self::Mooshroom | Self::Wither => 0.9,
            Self::Chicken | Self::Endermite => 0.4,
            Self::Silverfish => 0.3,
            Self::Guardian => 0.85,
            Self::Giant => 3.6,
            Self::Ghast => 4.0,
            Self::Arrow => 0.5,
            Self::Item => 0.25,
            _ => 0.6,
        }
    }

    /// True for living entities, which have a body yaw and swing their limbs;
    /// false for the objects (dropped items, arrows, item frames).
    #[must_use]
    pub const fn is_living(self) -> bool {
        !matches!(
            self,
            Self::Item
                | Self::Arrow
                | Self::ItemFrame
                | Self::FishHook
                | Self::Boat
                | Self::Minecart
                | Self::PrimedTnt
                | Self::EnderCrystal
                | Self::Snowball
                | Self::Egg
                | Self::LargeFireball
                | Self::SmallFireball
                | Self::EnderPearl
                | Self::WitherSkull
                | Self::FallingBlock
                | Self::EyeOfEnder
                | Self::Potion
                | Self::ExpBottle
                | Self::FireworkRocket
                | Self::LeashKnot
                | Self::XpOrb
                | Self::Painting
                | Self::Lightning
        )
    }

    /// Maps a 1.8 mob type id to an [`EntityKind`].
    #[must_use]
    pub const fn from_mob_type(id: u8) -> Self {
        match id {
            50 => Self::Creeper,
            51 => Self::Skeleton,
            52 => Self::Spider,
            53 => Self::Giant,
            54 => Self::Zombie,
            55 => Self::Slime,
            56 => Self::Ghast,
            57 => Self::ZombiePigman,
            58 => Self::Enderman,
            59 => Self::CaveSpider,
            60 => Self::Silverfish,
            61 => Self::Blaze,
            62 => Self::MagmaCube,
            63 => Self::Dragon,
            64 => Self::Wither,
            65 => Self::Bat,
            66 => Self::Witch,
            67 => Self::Endermite,
            68 => Self::Guardian,
            90 => Self::Pig,
            91 => Self::Sheep,
            92 => Self::Cow,
            93 => Self::Chicken,
            94 => Self::Squid,
            95 => Self::Wolf,
            96 => Self::Mooshroom,
            97 => Self::Snowman,
            98 => Self::Ocelot,
            99 => Self::IronGolem,
            100 => Self::Horse,
            101 => Self::Rabbit,
            120 => Self::Villager,
            other => Self::Other(other),
        }
    }

    /// Maps a 1.8 object type id to an [`EntityKind`].
    #[must_use]
    pub const fn from_object_type(id: u8) -> Self {
        match id {
            1 => Self::Boat,
            2 => Self::Item,
            10 => Self::Minecart,
            50 => Self::PrimedTnt,
            51 => Self::EnderCrystal,
            60 => Self::Arrow,
            61 => Self::Snowball,
            62 => Self::Egg,
            63 => Self::LargeFireball,
            64 => Self::SmallFireball,
            65 => Self::EnderPearl,
            66 => Self::WitherSkull,
            70 => Self::FallingBlock,
            71 => Self::ItemFrame,
            72 => Self::EyeOfEnder,
            73 => Self::Potion,
            75 => Self::ExpBottle,
            76 => Self::FireworkRocket,
            77 => Self::LeashKnot,
            78 => Self::ArmorStand,
            90 => Self::FishHook,
            other => Self::Other(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_put_the_tag_above_the_head() {
        assert_eq!(EntityKind::Player.height(), 1.8);
        assert_eq!(EntityKind::Enderman.height(), 2.9);
        assert!(EntityKind::Pig.height() < EntityKind::Cow.height());
    }

    #[test]
    fn widths_match_the_vanilla_footprints() {
        assert_eq!(EntityKind::Player.width(), 0.6);
        assert_eq!(EntityKind::Spider.width(), 1.4);
        assert_eq!(EntityKind::Chicken.width(), 0.4);
    }

    #[test]
    fn a_rabbit_is_as_tall_as_an_ocelot_not_a_cave_spider() {
        assert_eq!(EntityKind::Rabbit.height(), 0.7);
        assert_eq!(EntityKind::CaveSpider.height(), 0.5);
    }

    #[test]
    fn armour_stands_are_object_type_78() {
        assert_eq!(EntityKind::from_object_type(78), EntityKind::ArmorStand);
    }

    #[test]
    fn bobbers_are_object_type_90() {
        assert_eq!(EntityKind::from_object_type(90), EntityKind::FishHook);
        assert!(!EntityKind::FishHook.is_living());
    }

    #[test]
    fn item_frames_are_object_type_71() {
        assert_eq!(EntityKind::from_object_type(71), EntityKind::ItemFrame);
        assert_eq!(EntityKind::from_object_type(2), EntityKind::Item);
    }

    #[test]
    fn every_spawn_object_type_of_1_8_has_a_kind() {
        let table = [
            (1, EntityKind::Boat),
            (10, EntityKind::Minecart),
            (50, EntityKind::PrimedTnt),
            (51, EntityKind::EnderCrystal),
            (61, EntityKind::Snowball),
            (62, EntityKind::Egg),
            (63, EntityKind::LargeFireball),
            (64, EntityKind::SmallFireball),
            (65, EntityKind::EnderPearl),
            (66, EntityKind::WitherSkull),
            (70, EntityKind::FallingBlock),
            (72, EntityKind::EyeOfEnder),
            (73, EntityKind::Potion),
            (75, EntityKind::ExpBottle),
            (76, EntityKind::FireworkRocket),
            (77, EntityKind::LeashKnot),
        ];
        for (id, kind) in table {
            assert_eq!(EntityKind::from_object_type(id), kind);
            assert!(!kind.is_living());
        }
    }
}
