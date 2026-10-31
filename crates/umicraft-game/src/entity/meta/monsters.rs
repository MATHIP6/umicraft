// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slime, creeper, skeleton, zombie, enderman and villager entries.

use umicraft_protocol::packets::play::Metadata;

use super::Variant;

/// Slime entry: size (MCP `EntitySlime.entityInit`).
const SLIME_SIZE: u8 = 16;
/// Creeper entries: state, powered, ignited (MCP `EntityCreeper.entityInit`).
const CREEPER_STATE: u8 = 16;
const CREEPER_POWERED: u8 = 17;
const CREEPER_IGNITED: u8 = 18;
/// Skeleton entry: type, `1` for a wither skeleton.
const SKELETON_TYPE: u8 = 13;
/// Zombie entries: child, villager, converting (MCP `EntityZombie`).
const ZOMBIE_CHILD: u8 = 12;
const ZOMBIE_VILLAGER: u8 = 13;
const ZOMBIE_CONVERTING: u8 = 14;
/// Enderman entries: carried block state, screaming.
const GHAST_ATTACKING: u8 = 16;
const WITHER_INVULNERABLE: u8 = 20;
const ENDERMAN_CARRIED: u8 = 16;
const ENDERMAN_SCREAMING: u8 = 18;
/// Villager entry: profession.
const VILLAGER_PROFESSION: u8 = 16;

/// Creeper state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreeperMeta {
    /// `-1` idle, `1` fusing (MCP `getCreeperState`).
    pub state: i8,
    /// Struck by lightning: a charged creeper.
    pub powered: bool,
    /// Lit by flint and steel.
    pub ignited: bool,
}

impl Default for CreeperMeta {
    fn default() -> Self {
        Self {
            state: -1,
            powered: false,
            ignited: false,
        }
    }
}

/// Zombie state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZombieMeta {
    /// A baby zombie (`None` until the entry is seen).
    pub child: Option<bool>,
    /// A zombie villager.
    pub villager: bool,
    /// Being cured of the villager state.
    pub converting: bool,
}

/// Enderman state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndermanMeta {
    /// Carried block state (`id | metadata << 12`), `0` for none.
    pub carried: u16,
    /// Staring at a player: the mouth opens.
    pub screaming: bool,
}

pub(super) fn apply_wither(variant: &mut Variant, metadata: &Metadata) {
    if let Some(invulnerable) = metadata.get_int(WITHER_INVULNERABLE) {
        *variant = Variant::Wither { invulnerable };
    }
}

/// Entries of the shake of a boat or a minecart (`EntityBoat.entityInit`,
/// `EntityMinecart.entityInit`): ticks left, direction and damage.
const VEHICLE_AMPLITUDE: u8 = 17;
const VEHICLE_DIRECTION: u8 = 18;
const VEHICLE_DAMAGE: u8 = 19;

pub(super) fn apply_vehicle(variant: &mut Variant, metadata: &Metadata) {
    let (mut amplitude, mut direction, mut damage) = match *variant {
        Variant::Vehicle {
            amplitude,
            direction,
            damage,
        } => (amplitude, direction, damage),
        _ => (0, 1, 0.0),
    };
    if let Some(value) = metadata.get_int(VEHICLE_AMPLITUDE) {
        amplitude = value;
    }
    if let Some(value) = metadata.get_int(VEHICLE_DIRECTION) {
        direction = value;
    }
    if let Some(value) = metadata.get_float(VEHICLE_DAMAGE) {
        damage = value;
    }
    *variant = Variant::Vehicle {
        amplitude,
        direction,
        damage,
    };
}

/// One tick of the shake dying down (`EntityMinecart.onUpdate`,
/// `EntityBoat.onUpdate`).
pub(super) fn tick_vehicle(variant: &mut Variant) {
    if let Variant::Vehicle {
        amplitude, damage, ..
    } = variant
    {
        if *amplitude > 0 {
            *amplitude -= 1;
        }
        if *damage > 0.0 {
            *damage -= 1.0;
        }
    }
}

/// Data watcher entry of `EntityWitherSkull.invulnerable`.
const WITHER_SKULL_INVULNERABLE: u8 = 10;

pub(super) fn apply_wither_skull(variant: &mut Variant, metadata: &Metadata) {
    if let Some(invulnerable) = metadata.get_byte(WITHER_SKULL_INVULNERABLE) {
        *variant = Variant::WitherSkull {
            invulnerable: invulnerable != 0,
        };
    }
}

pub(super) fn apply_ghast(variant: &mut Variant, metadata: &Metadata) {
    if let Some(attacking) = metadata.get_byte(GHAST_ATTACKING) {
        *variant = Variant::Ghast {
            attacking: attacking != 0,
        };
    }
}

pub(super) fn apply_slime(variant: &mut Variant, metadata: &Metadata) {
    if let Some(size) = metadata.get_byte(SLIME_SIZE) {
        *variant = Variant::Slime(size.max(1));
    }
}

pub(super) fn apply_creeper(variant: &mut Variant, metadata: &Metadata) {
    let mut creeper = match variant {
        Variant::Creeper(creeper) => *creeper,
        _ => CreeperMeta::default(),
    };
    if let Some(state) = metadata.get_byte(CREEPER_STATE) {
        creeper.state = state as i8;
    }
    if let Some(powered) = metadata.get_byte(CREEPER_POWERED) {
        creeper.powered = powered != 0;
    }
    if let Some(ignited) = metadata.get_byte(CREEPER_IGNITED) {
        creeper.ignited = ignited != 0;
    }
    *variant = Variant::Creeper(creeper);
}

pub(super) fn apply_skeleton(variant: &mut Variant, metadata: &Metadata) {
    if let Some(kind) = metadata.get_byte(SKELETON_TYPE) {
        *variant = Variant::Skeleton { wither: kind == 1 };
    }
}

pub(super) fn apply_zombie(variant: &mut Variant, metadata: &Metadata) {
    let mut zombie = match variant {
        Variant::Zombie(zombie) => *zombie,
        _ => ZombieMeta::default(),
    };
    if let Some(child) = metadata.get_byte(ZOMBIE_CHILD) {
        zombie.child = Some(child == 1);
    }
    if let Some(villager) = metadata.get_byte(ZOMBIE_VILLAGER) {
        zombie.villager = villager == 1;
    }
    if let Some(converting) = metadata.get_byte(ZOMBIE_CONVERTING) {
        zombie.converting = converting == 1;
    }
    *variant = Variant::Zombie(zombie);
}

pub(super) fn apply_enderman(variant: &mut Variant, metadata: &Metadata) {
    let mut enderman = match variant {
        Variant::Enderman(enderman) => *enderman,
        _ => EndermanMeta::default(),
    };
    if let Some(carried) = metadata.get_short(ENDERMAN_CARRIED) {
        enderman.carried = carried as u16;
    }
    if let Some(screaming) = metadata.get_byte(ENDERMAN_SCREAMING) {
        enderman.screaming = screaming != 0;
    }
    *variant = Variant::Enderman(enderman);
}

pub(super) fn apply_villager(variant: &mut Variant, metadata: &Metadata) {
    if let Some(profession) = metadata.get_int(VILLAGER_PROFESSION) {
        *variant = Variant::Villager { profession };
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{byte, int, meta, short};
    use super::super::EntityMeta;
    use super::*;
    use crate::entity::kind::EntityKind;

    #[test]
    fn a_slime_has_a_size() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Slime, &meta(vec![byte(16, 4)]));
        assert_eq!(state.variant, Variant::Slime(4));
        state.apply(EntityKind::MagmaCube, &meta(vec![byte(16, 0)]));
        assert_eq!(state.variant, Variant::Slime(1), "never smaller than one");
    }

    #[test]
    fn a_creeper_fuses_and_charges() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Creeper, &meta(vec![byte(16, 1), byte(17, 1)]));
        assert_eq!(
            state.variant,
            Variant::Creeper(CreeperMeta {
                state: 1,
                powered: true,
                ignited: false
            })
        );
        state.apply(EntityKind::Creeper, &meta(vec![byte(16, 0xFF)]));
        assert!(matches!(state.variant, Variant::Creeper(c) if c.state == -1 && c.powered));
    }

    #[test]
    fn a_skeleton_can_be_a_wither_skeleton() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Skeleton, &meta(vec![byte(13, 1)]));
        assert_eq!(state.variant, Variant::Skeleton { wither: true });
    }

    #[test]
    fn a_zombie_can_be_a_baby_villager() {
        let mut state = EntityMeta::default();
        state.apply(
            EntityKind::Zombie,
            &meta(vec![byte(12, 1), byte(13, 1), byte(14, 1)]),
        );
        assert!(state.baby);
        assert!(state.is_zombie_villager());
        assert!(matches!(state.variant, Variant::Zombie(z) if z.converting));
        state.apply(EntityKind::Zombie, &meta(vec![byte(12, 0)]));
        assert!(!state.baby);
    }

    #[test]
    fn a_villager_has_a_profession() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Villager, &meta(vec![int(16, 3)]));
        assert_eq!(state.variant, Variant::Villager { profession: 3 });
    }

    #[test]
    fn an_enderman_carries_a_block_and_screams() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Enderman, &meta(vec![short(16, 2), byte(18, 1)]));
        assert_eq!(
            state.variant,
            Variant::Enderman(EndermanMeta {
                carried: 2,
                screaming: true
            })
        );
    }
}
