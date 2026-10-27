// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure game-level entity types, identifiers and render snapshots.

use glam::{DVec3, Vec3};

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
    /// Other mob or entity type by raw numeric identifier.
    Other(u8),
}

impl EntityKind {
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
            other => Self::Other(other),
        }
    }
}

/// Bit flags representing active entity visual states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityFlags {
    /// Raw byte flags (index 0 in 1.8 data watchers).
    pub flags: u8,
    /// Player skin parts bitmask (index 10 in 1.8 data watchers).
    pub skin_parts: u8,
}

impl EntityFlags {
    /// Creates flags from raw data watcher bytes.
    #[must_use]
    pub const fn new(flags: u8, skin_parts: u8) -> Self {
        Self { flags, skin_parts }
    }

    /// Returns true if the entity is burning.
    #[must_use]
    pub const fn on_fire(self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Returns true if the entity is crouching / sneaking.
    #[must_use]
    pub const fn is_sneaking(self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Returns true if the entity is sprinting.
    #[must_use]
    pub const fn is_sprinting(self) -> bool {
        self.flags & 0x08 != 0
    }

    /// Returns true if the entity is invisible.
    #[must_use]
    pub const fn is_invisible(self) -> bool {
        self.flags & 0x20 != 0
    }
}

/// Full mutable record of an entity owned by the game thread.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityRecord {
    /// Unique entity ID.
    pub id: EntityId,
    /// Entity category.
    pub kind: EntityKind,
    /// Current world position of the entity's feet.
    pub position: DVec3,
    /// Position at the previous simulation tick.
    pub previous_position: DVec3,
    /// Server-authoritative target position when interpolating movement packets.
    pub target_position: DVec3,
    /// Velocity in blocks per tick.
    pub velocity: Vec3,
    /// Body horizontal yaw in degrees.
    pub yaw: f32,
    /// Previous body yaw for interpolation.
    pub previous_yaw: f32,
    /// Target body yaw.
    pub target_yaw: f32,
    /// Vertical pitch in degrees.
    pub pitch: f32,
    /// Previous vertical pitch.
    pub previous_pitch: f32,
    /// Target vertical pitch.
    pub target_pitch: f32,
    /// Head yaw in degrees.
    pub head_yaw: f32,
    /// Previous head yaw.
    pub previous_head_yaw: f32,
    /// Target head yaw.
    pub target_head_yaw: f32,
    /// Whether the entity rests on the ground.
    pub on_ground: bool,
    /// Accumulated limb swing animation progress in radians.
    pub limb_swing: f32,
    /// Current limb swing amplitude (0.0 = still, 1.0 = full walk speed).
    pub limb_swing_amount: f32,
    /// Previous limb swing amplitude.
    pub previous_limb_swing_amount: f32,
    /// Player UUID if this is a player entity.
    pub uuid: Option<[u8; 16]>,
    /// Numeric ID of the held item (or 0 for empty hand).
    pub held_item: Option<u16>,
    /// Visual flags.
    pub flags: EntityFlags,
    /// Entity health from metadata index 6 (if present).
    pub health: Option<f32>,
    /// Remaining interpolation ticks for position/rotation smoothing (vanilla: 3).
    pub interpolation_steps: u8,
}

impl EntityRecord {
    /// Creates a new entity record at the given initial position.
    #[must_use]
    pub fn new(id: EntityId, kind: EntityKind, position: DVec3, yaw: f32, pitch: f32) -> Self {
        Self {
            id,
            kind,
            position,
            previous_position: position,
            target_position: position,
            velocity: Vec3::ZERO,
            yaw,
            previous_yaw: yaw,
            target_yaw: yaw,
            pitch,
            previous_pitch: pitch,
            target_pitch: pitch,
            head_yaw: yaw,
            previous_head_yaw: yaw,
            target_head_yaw: yaw,
            on_ground: true,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            previous_limb_swing_amount: 0.0,
            uuid: None,
            held_item: None,
            flags: EntityFlags::default(),
            health: None,
            interpolation_steps: 0,
        }
    }
}

/// Per-entity interpolation data sent to the render thread each tick.
///
/// Carries the previous and current tick states so the render thread can
/// produce smoothly interpolated snapshots at any sub-tick fraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityFrameData {
    /// Unique entity ID.
    pub id: EntityId,
    /// Entity category.
    pub kind: EntityKind,
    /// Position at the previous simulation tick.
    pub prev_position: Vec3,
    /// Position at the current simulation tick.
    pub position: Vec3,
    /// Previous body yaw in degrees.
    pub prev_yaw: f32,
    /// Current body yaw in degrees.
    pub yaw: f32,
    /// Previous pitch in degrees.
    pub prev_pitch: f32,
    /// Current pitch in degrees.
    pub pitch: f32,
    /// Previous head yaw in degrees.
    pub prev_head_yaw: f32,
    /// Current head yaw in degrees.
    pub head_yaw: f32,
    /// Accumulated limb swing phase.
    pub limb_swing: f32,
    /// Previous limb swing amplitude.
    pub prev_limb_swing_amount: f32,
    /// Current limb swing amplitude.
    pub limb_swing_amount: f32,
    /// Player UUID for skin lookup.
    pub uuid: Option<[u8; 16]>,
    /// Numeric ID of the held item.
    pub held_item: Option<u16>,
    /// Entity visual flags.
    pub flags: EntityFlags,
}

impl EntityFrameData {
    /// Produces an interpolated render snapshot at the given sub-tick fraction.
    ///
    /// `alpha` is in `0.0..=1.0` where 0 is the previous tick and 1 the current.
    #[must_use]
    pub fn interpolate(&self, alpha: f32) -> EntityRenderSnapshot {
        let a = alpha.clamp(0.0, 1.0);
        let position = self.prev_position.lerp(self.position, a);
        let yaw = interpolate_angle_deg(self.prev_yaw, self.yaw, a);
        let pitch = self.prev_pitch + (self.pitch - self.prev_pitch) * a;
        let head_yaw = interpolate_angle_deg(self.prev_head_yaw, self.head_yaw, a);
        let limb_swing_amount = self.prev_limb_swing_amount
            + (self.limb_swing_amount - self.prev_limb_swing_amount) * a;
        let limb_swing = self.limb_swing - self.limb_swing_amount * (1.0 - a);

        EntityRenderSnapshot {
            id: self.id,
            kind: self.kind,
            position,
            yaw,
            pitch,
            head_yaw,
            limb_swing,
            limb_swing_amount,
            uuid: self.uuid,
            held_item: self.held_item,
            flags: self.flags,
        }
    }
}

/// Interpolates between two degree angles taking the shortest arc.
fn interpolate_angle_deg(prev: f32, current: f32, alpha: f32) -> f32 {
    let mut diff = (current - prev) % 360.0;
    if diff < -180.0 {
        diff += 360.0;
    } else if diff >= 180.0 {
        diff -= 360.0;
    }
    prev + diff * alpha
}

/// Immutable snapshot of one entity prepared for the render thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityRenderSnapshot {
    /// Unique entity ID.
    pub id: EntityId,
    /// Entity category.
    pub kind: EntityKind,
    /// Interpolated world space position.
    pub position: Vec3,
    /// Interpolated body yaw in degrees.
    pub yaw: f32,
    /// Interpolated pitch in degrees.
    pub pitch: f32,
    /// Interpolated head yaw in degrees.
    pub head_yaw: f32,
    /// Interpolated limb swing progress in radians.
    pub limb_swing: f32,
    /// Interpolated limb swing amplitude (0.0 to 1.0).
    pub limb_swing_amount: f32,
    /// Player UUID for skin lookup.
    pub uuid: Option<[u8; 16]>,
    /// Numeric ID of the held item.
    pub held_item: Option<u16>,
    /// Entity visual flags.
    pub flags: EntityFlags,
}
