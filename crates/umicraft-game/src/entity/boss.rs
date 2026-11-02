// SPDX-License-Identifier: MIT OR Apache-2.0

//! Boss bar tracking (M10, UI-2).
//!
//! In Minecraft 1.8 there is no dedicated boss bar packet: the boss bar is
//! driven by active boss entities in the world (the Ender Dragon and the
//! Wither), whose current health is updated via entity metadata (data watcher
//! index 6, float).
//!
//! MCP reference: `EntityDragon` has 200 max health; `EntityWither` has 300
//! max health; `BossStatus.java` renders the health scale and formatted boss
//! name.

use super::kind::EntityKind;
use super::store::EntityStore;

/// Maximum health of an Ender Dragon in Minecraft 1.8.
pub const DRAGON_MAX_HEALTH: f32 = 200.0;

/// Maximum health of a Wither in Minecraft 1.8.
pub const WITHER_MAX_HEALTH: f32 = 300.0;

/// Language key of the Ender Dragon's name (`EntityList` string `EnderDragon`,
/// `Entity.getName`: `"entity." + string + ".name"`).
pub const DRAGON_NAME_KEY: &str = "entity.EnderDragon.name";

/// Language key of the Wither's name (`EntityList` string `WitherBoss`).
pub const WITHER_NAME_KEY: &str = "entity.WitherBoss.name";

/// Active boss information suitable for rendering on the HUD.
#[derive(Debug, Clone, PartialEq)]
pub struct BossInfo {
    /// The language key of the boss type; the HUD translates it.
    pub name_key: &'static str,
    /// The custom name of the boss, which replaces the translated one
    /// (`Entity.getName`).
    pub custom_name: Option<String>,
    /// Health fraction in `0.0..=1.0`.
    pub health_fraction: f32,
}

/// Tracks active boss entities in the world.
#[derive(Debug, Clone, Default)]
pub struct BossTracker;

impl BossTracker {
    /// Discovers the active boss in `store`, if any.
    ///
    /// If multiple bosses are present, returns the first found (prioritising
    /// the Ender Dragon then the Wither).
    #[must_use]
    pub fn find_boss(store: &EntityStore) -> Option<BossInfo> {
        let mut dragon = None;
        let mut wither = None;

        for record in store.entities().values() {
            match record.kind {
                EntityKind::Dragon => {
                    let health = record.health.unwrap_or(DRAGON_MAX_HEALTH);
                    dragon = Some(BossInfo {
                        name_key: DRAGON_NAME_KEY,
                        custom_name: record.meta.custom_name.as_deref().map(str::to_owned),
                        health_fraction: (health / DRAGON_MAX_HEALTH).clamp(0.0, 1.0),
                    });
                    break;
                }
                EntityKind::Wither if wither.is_none() => {
                    let health = record.health.unwrap_or(WITHER_MAX_HEALTH);
                    wither = Some(BossInfo {
                        name_key: WITHER_NAME_KEY,
                        custom_name: record.meta.custom_name.as_deref().map(str::to_owned),
                        health_fraction: (health / WITHER_MAX_HEALTH).clamp(0.0, 1.0),
                    });
                }
                _ => {}
            }
        }

        dragon.or(wither)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{EntityFlags, EntityId};
    use glam::{DVec3, Vec3};

    #[test]
    fn empty_store_has_no_boss() {
        let store = EntityStore::new();
        assert_eq!(BossTracker::find_boss(&store), None);
    }

    #[test]
    fn dragon_and_wither_boss_tracking() {
        let mut store = EntityStore::new();
        // Mob type 64 = Wither
        store.spawn_mob(
            EntityId(10),
            64,
            DVec3::ZERO,
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
        let boss = BossTracker::find_boss(&store).expect("wither boss");
        assert_eq!(boss.name_key, WITHER_NAME_KEY);
        assert_eq!(boss.health_fraction, 1.0);

        store.set_health(EntityId(10), 150.0);
        let boss = BossTracker::find_boss(&store).expect("wither boss damaged");
        assert_eq!(boss.health_fraction, 0.5);

        // Mob type 63 = Dragon
        store.spawn_mob(
            EntityId(20),
            63,
            DVec3::ZERO,
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
        store.set_health(EntityId(20), 50.0);
        let boss = BossTracker::find_boss(&store).expect("dragon prioritized");
        assert_eq!(boss.name_key, DRAGON_NAME_KEY);
        assert_eq!(boss.health_fraction, 0.25);

        store.destroy(&[20]);
        let boss = BossTracker::find_boss(&store).expect("wither after dragon destroyed");
        assert_eq!(boss.name_key, WITHER_NAME_KEY);
        assert_eq!(boss.health_fraction, 0.5);
    }
}
