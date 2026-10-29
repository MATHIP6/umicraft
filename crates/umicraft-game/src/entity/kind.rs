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
    /// Other mob or entity type by raw numeric identifier.
    Other(u8),
}

impl EntityKind {
    /// True for living entities, which have a body yaw and swing their limbs;
    /// false for the objects (dropped items, arrows, item frames).
    #[must_use]
    pub const fn is_living(self) -> bool {
        !matches!(self, Self::Item | Self::Arrow | Self::ItemFrame)
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
            2 => Self::Item,
            60 => Self::Arrow,
            71 => Self::ItemFrame,
            78 => Self::ArmorStand,
            other => Self::Other(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_frames_are_object_type_71() {
        assert_eq!(EntityKind::from_object_type(71), EntityKind::ItemFrame);
        assert_eq!(EntityKind::from_object_type(2), EntityKind::Item);
    }
}
