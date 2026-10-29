// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed entity metadata: the 1.8 data watcher entries each entity class
//! defines (MCP `entityInit`), decoded into plain fields.
//!
//! An `Entity Metadata` packet carries only the entries that changed, so
//! [`EntityMeta::apply`] updates a field only when its entry is present.
//! One submodule decodes each family of classes.

mod animals;
mod armor_stand;
mod common;
mod horse;
mod monsters;

use std::sync::Arc;

use umicraft_protocol::packets::play::Metadata;

use super::kind::EntityKind;

pub use animals::{OcelotMeta, RabbitMeta, SheepMeta, WolfMeta};
pub use armor_stand::{ArmorStandMeta, Pose};
pub use common::LivingMeta;
pub use horse::HorseMeta;
pub use monsters::{CreeperMeta, EndermanMeta, ZombieMeta};

/// The decoded metadata of one entity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityMeta {
    /// Custom name (entry 2), shown above the entity.
    pub custom_name: Option<Arc<str>>,
    /// Whether the custom name shows without looking at the entity (entry 3).
    pub custom_name_visible: bool,
    /// Health, potion colour, arrows in the body, absorption.
    pub living: LivingMeta,
    /// A child of an ageable mob or a baby zombie.
    pub baby: bool,
    /// What the entity's class adds.
    pub variant: Variant,
}

/// The class-specific metadata of an entity.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Variant {
    /// The class defines no entry that changes the look.
    #[default]
    None,
    /// Sheep colour and shearing.
    Sheep(SheepMeta),
    /// Slime and magma cube size (`1`, `2`, `4`).
    Slime(u8),
    /// Creeper fuse, charge and ignition.
    Creeper(CreeperMeta),
    /// Wither skeleton flag (entry 13 is `1`).
    Skeleton {
        /// True for a wither skeleton.
        wither: bool,
    },
    /// Zombie villager and conversion flags.
    Zombie(ZombieMeta),
    /// Villager profession (entry 16).
    Villager {
        /// `0` farmer, `1` librarian, `2` priest, `3` smith, `4` butcher.
        profession: i32,
    },
    /// Horse type, coat and armour.
    Horse(HorseMeta),
    /// Wolf taming, anger and collar.
    Wolf(WolfMeta),
    /// Ocelot taming and coat.
    Ocelot(OcelotMeta),
    /// Rabbit coat.
    Rabbit(RabbitMeta),
    /// Pig saddle (entry 16).
    Pig {
        /// True when the pig wears a saddle.
        saddled: bool,
    },
    /// Enderman carried block and scream.
    Enderman(EndermanMeta),
    /// Armour stand flags and pose.
    ArmorStand(ArmorStandMeta),
}

impl EntityMeta {
    /// Merges the entries of `metadata` into this state; `kind` selects which
    /// class-specific entries exist.
    pub fn apply(&mut self, kind: EntityKind, metadata: &Metadata) {
        common::apply(self, kind, metadata);
        let mut variant = std::mem::take(&mut self.variant);
        match kind {
            EntityKind::Sheep => animals::apply_sheep(&mut variant, metadata),
            EntityKind::Wolf => animals::apply_wolf(&mut variant, metadata),
            EntityKind::Ocelot => animals::apply_ocelot(&mut variant, metadata),
            EntityKind::Rabbit => animals::apply_rabbit(&mut variant, metadata),
            EntityKind::Pig => animals::apply_pig(&mut variant, metadata),
            EntityKind::Horse => horse::apply(&mut variant, metadata),
            EntityKind::Slime | EntityKind::MagmaCube => {
                monsters::apply_slime(&mut variant, metadata);
            }
            EntityKind::Creeper => monsters::apply_creeper(&mut variant, metadata),
            EntityKind::Skeleton => monsters::apply_skeleton(&mut variant, metadata),
            EntityKind::Zombie | EntityKind::ZombiePigman | EntityKind::ZombieVillager => {
                monsters::apply_zombie(&mut variant, metadata);
            }
            EntityKind::Enderman => monsters::apply_enderman(&mut variant, metadata),
            EntityKind::Villager => monsters::apply_villager(&mut variant, metadata),
            EntityKind::ArmorStand => armor_stand::apply(&mut variant, metadata),
            _ => {}
        }
        self.variant = variant;
        if let Variant::Zombie(zombie) = &self.variant {
            if let Some(child) = zombie.child {
                self.baby = child;
            }
        }
    }

    /// True when a zombie is a zombie villager (entry 13), whatever its
    /// entity type says.
    #[must_use]
    pub fn is_zombie_villager(&self) -> bool {
        matches!(&self.variant, Variant::Zombie(zombie) if zombie.villager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::metadata::WatchedObject;

    pub(super) fn byte(index: u8, value: u8) -> WatchedObject {
        WatchedObject {
            type_id: 0,
            index,
            value: vec![value],
        }
    }

    pub(super) fn int(index: u8, value: i32) -> WatchedObject {
        WatchedObject {
            type_id: 2,
            index,
            value: value.to_be_bytes().to_vec(),
        }
    }

    pub(super) fn float(index: u8, value: f32) -> WatchedObject {
        WatchedObject {
            type_id: 3,
            index,
            value: value.to_be_bytes().to_vec(),
        }
    }

    pub(super) fn short(index: u8, value: i16) -> WatchedObject {
        WatchedObject {
            type_id: 1,
            index,
            value: value.to_be_bytes().to_vec(),
        }
    }

    pub(super) fn string(index: u8, value: &str) -> WatchedObject {
        let mut bytes = vec![u8::try_from(value.len()).expect("short test string")];
        bytes.extend_from_slice(value.as_bytes());
        WatchedObject {
            type_id: 4,
            index,
            value: bytes,
        }
    }

    pub(super) fn meta(objects: Vec<WatchedObject>) -> Metadata {
        Metadata { objects }
    }

    #[test]
    fn an_update_keeps_the_entries_it_does_not_carry() {
        let mut state = EntityMeta::default();
        state.apply(
            EntityKind::Sheep,
            &meta(vec![byte(16, 0x1A), string(2, "Dolly")]),
        );
        state.apply(EntityKind::Sheep, &meta(vec![float(6, 4.0)]));
        assert_eq!(state.custom_name.as_deref(), Some("Dolly"));
        assert_eq!(state.living.health, 4.0);
        assert!(
            matches!(state.variant, Variant::Sheep(sheep) if sheep.color == 10 && sheep.sheared)
        );
    }

    #[test]
    fn a_class_without_entries_keeps_no_variant() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Cow, &meta(vec![byte(12, 0xFF)]));
        assert_eq!(state.variant, Variant::None);
        assert!(state.baby, "a negative age byte is a calf");
    }
}
