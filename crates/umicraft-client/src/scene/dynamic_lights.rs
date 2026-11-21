// SPDX-License-Identifier: MIT OR Apache-2.0

//! Finds the light sources of a frame for the Dynamic Lights option: the
//! entities that glow (burning, a blaze, a held torch, a dropped glowstone).
//!
//! The rules and the item table follow OptiFine's `DynamicLights.getLightLevel`
//! for an entity and for an item stack (ADR-0088, `docs/sources.md`). The
//! collector refreshes its list on OptiFine's schedule: `DynamicLights.update`
//! runs every 50 ms (one game tick) and Fast mode lets each light update only
//! every 500 ms (`DynamicLight.update`). Between refreshes the previous list
//! is kept as it is, so a Fast light trails its entity. Nothing allocates: the
//! list is the renderer's fixed-size [`DynamicLightSet`].

use std::time::{Duration, Instant};

use glam::DVec3;
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_render::{DynamicLight, DynamicLightSet};
use umicraft_world::{light_emission, BlockState};

pub use crate::options::video_sections::OffFastFancy;

/// How often Fancy refreshes the lights: `DynamicLights.update` runs once
/// every 50 ms.
const FANCY_INTERVAL: Duration = Duration::from_millis(50);

/// How often Fast refreshes them (`DynamicLight.update` returns early inside
/// 500 ms of the previous check in Fast mode).
const FAST_INTERVAL: Duration = Duration::from_millis(500);

/// Block ids below this are the items of the same id (`ItemBlock`).
const FIRST_NON_BLOCK_ITEM: i16 = 256;

/// Light of a burning entity, a fireball and primed TNT.
const FIRE_LEVEL: u8 = 15;
/// A blaze, which `func_70845_n` (its charged state, not in the metadata the
/// client keeps) would raise to [`FIRE_LEVEL`].
const BLAZE_LEVEL: u8 = 10;
/// A magma cube at rest; squashed past [`MAGMA_CUBE_SQUASH`] it shows its core.
const MAGMA_CUBE_LEVEL: u8 = 8;
const MAGMA_CUBE_CORE_LEVEL: u8 = 13;
/// The squish factor above which a magma cube shines with its core.
const MAGMA_CUBE_SQUASH: f32 = 0.6;
/// A creeper that flashes (`getCreeperFlashIntensity(0) > 0.001`).
const CREEPER_FLASH: f32 = 0.001;

/// The 1.8 item ids that glow without being blocks.
mod item {
    pub(super) const GLOWSTONE_DUST: i16 = 348;
    pub(super) const LAVA_BUCKET: i16 = 327;
    pub(super) const BLAZE_ROD: i16 = 369;
    pub(super) const BLAZE_POWDER: i16 = 377;
    pub(super) const MAGMA_CREAM: i16 = 378;
    pub(super) const NETHER_STAR: i16 = 399;
    pub(super) const PRISMARINE_CRYSTALS: i16 = 410;
}

/// The light an item gives (`DynamicLights.getLightLevel(ItemStack)`): a
/// block item gives its block's light, and a few items glow by themselves.
#[must_use]
pub fn item_light(item_id: i16) -> u8 {
    if item_id < FIRST_NON_BLOCK_ITEM {
        return u16::try_from(item_id).map_or(0, |block| light_emission(BlockState::new(block, 0)));
    }
    match item_id {
        // `Blocks.lava.getLightValue()`.
        item::LAVA_BUCKET => 15,
        item::BLAZE_ROD | item::BLAZE_POWDER => 10,
        item::GLOWSTONE_DUST | item::PRISMARINE_CRYSTALS | item::MAGMA_CREAM => 8,
        // `Blocks.beacon.getLightValue() / 2`.
        item::NETHER_STAR => 7,
        _ => 0,
    }
}

/// The light of an optional stack.
fn stack_light(stack: Option<&EntityItem>) -> u8 {
    stack.map_or(0, |stack| item_light(stack.item_id))
}

/// The light an entity gives (`DynamicLights.getLightLevel(Entity)`), `0`
/// when it gives none.
#[must_use]
pub fn entity_light(entity: &EntityRenderSnapshot) -> u8 {
    if entity.kind == EntityKind::Player && entity.spectator {
        return 0;
    }
    if entity.flags.on_fire() {
        return FIRE_LEVEL;
    }
    match entity.kind {
        EntityKind::LargeFireball
        | EntityKind::SmallFireball
        | EntityKind::WitherSkull
        | EntityKind::PrimedTnt => FIRE_LEVEL,
        EntityKind::Blaze => BLAZE_LEVEL,
        EntityKind::MagmaCube if entity.squish > MAGMA_CUBE_SQUASH => MAGMA_CUBE_CORE_LEVEL,
        EntityKind::MagmaCube => MAGMA_CUBE_LEVEL,
        EntityKind::Creeper if entity.fuse_flash > CREEPER_FLASH => FIRE_LEVEL,
        EntityKind::Item => stack_light(entity.item.as_ref()),
        kind if kind.is_living() => {
            // The helmet is the last of the armour, boots first.
            stack_light(entity.held.as_ref()).max(stack_light(entity.armor[3].as_ref()))
        }
        _ => 0,
    }
}

/// How high above its feet an entity's light shines (`Entity.getEyeHeight`:
/// a player's eyes, 85 % of a living entity's height, half of an object's).
fn eye_height(kind: EntityKind) -> f64 {
    let height = f64::from(kind.height());
    match kind {
        EntityKind::Player => 1.62,
        kind if kind.is_living() => height * 0.85,
        _ => height / 2.0,
    }
}

/// Collects the lights of the entities on OptiFine's schedule.
#[derive(Debug, Default)]
pub struct DynamicLightCollector {
    lights: DynamicLightSet,
    last_refresh: Option<Instant>,
}

impl DynamicLightCollector {
    /// The lights to draw this frame. `entities` are this frame's snapshots,
    /// `eye` is the camera (the nearest lights are kept when there are too
    /// many) and `viewer_underwater` says whether the camera is in water,
    /// which the local player's own light then is too.
    ///
    /// Off clears the list and costs nothing. Otherwise the list is rebuilt
    /// once per refresh interval and returned unchanged in between.
    pub fn update(
        &mut self,
        mode: OffFastFancy,
        now: Instant,
        entities: &[EntityRenderSnapshot],
        eye: DVec3,
        viewer_underwater: bool,
    ) -> &DynamicLightSet {
        let interval = match mode {
            OffFastFancy::Off => {
                self.lights.clear();
                self.last_refresh = None;
                return &self.lights;
            }
            OffFastFancy::Fast => FAST_INTERVAL,
            OffFastFancy::Fancy => FANCY_INTERVAL,
        };
        let due = self
            .last_refresh
            .is_none_or(|last| now.duration_since(last) >= interval);
        if due {
            self.last_refresh = Some(now);
            self.lights.clear();
            for entity in entities {
                let level = entity_light(entity);
                if level == 0 {
                    continue;
                }
                let feet = entity.origin + entity.position.as_dvec3();
                self.lights.push(
                    DynamicLight {
                        position: feet + DVec3::new(0.0, eye_height(entity.kind), 0.0),
                        level,
                        underwater: entity.local && viewer_underwater,
                    },
                    eye,
                );
            }
        }
        &self.lights
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use umicraft_game::entity::{EntityFlags, EntityId};

    use super::*;

    fn entity(kind: EntityKind) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(1.0, 64.0, 2.0))
    }

    fn stack(item_id: i16) -> Option<EntityItem> {
        Some(EntityItem {
            item_id,
            damage: 0,
            count: 1,
            nbt: None,
        })
    }

    #[test]
    fn block_items_give_the_light_of_their_block() {
        assert_eq!(item_light(50), 14, "torch");
        assert_eq!(item_light(89), 15, "glowstone");
        assert_eq!(item_light(76), 7, "redstone torch");
        assert_eq!(item_light(1), 0, "stone");
        assert_eq!(item_light(0), 0);
    }

    #[test]
    fn the_glowing_items_follow_the_reference_table() {
        for (id, level) in [
            (item::LAVA_BUCKET, 15),
            (item::BLAZE_ROD, 10),
            (item::BLAZE_POWDER, 10),
            (item::GLOWSTONE_DUST, 8),
            (item::PRISMARINE_CRYSTALS, 8),
            (item::MAGMA_CREAM, 8),
            (item::NETHER_STAR, 7),
        ] {
            assert_eq!(item_light(id), level, "item {id}");
        }
        assert_eq!(item_light(276), 0, "diamond sword");
        assert_eq!(item_light(-1), 0);
    }

    #[test]
    fn a_burning_entity_gives_full_light() {
        let mut pig = entity(EntityKind::Pig);
        assert_eq!(entity_light(&pig), 0);
        pig.flags = EntityFlags::new(0x01, 0);
        assert_eq!(entity_light(&pig), 15);
    }

    #[test]
    fn fireballs_and_primed_tnt_glow() {
        for kind in [
            EntityKind::LargeFireball,
            EntityKind::SmallFireball,
            EntityKind::PrimedTnt,
        ] {
            assert_eq!(entity_light(&entity(kind)), 15, "{kind:?}");
        }
    }

    #[test]
    fn mobs_that_glow_by_kind() {
        assert_eq!(entity_light(&entity(EntityKind::Blaze)), 10);
        let mut cube = entity(EntityKind::MagmaCube);
        assert_eq!(entity_light(&cube), 8);
        cube.squish = 0.7;
        assert_eq!(entity_light(&cube), 13);
        let mut creeper = entity(EntityKind::Creeper);
        assert_eq!(entity_light(&creeper), 0);
        creeper.fuse_flash = 0.5;
        assert_eq!(entity_light(&creeper), 15);
    }

    #[test]
    fn a_living_entity_gives_the_brighter_of_its_hand_and_helmet() {
        let mut player = entity(EntityKind::Player);
        player.held = stack(50);
        assert_eq!(entity_light(&player), 14);
        player.armor[3] = stack(89);
        assert_eq!(entity_light(&player), 15);
        player.armor[3] = None;
        player.held = stack(item::MAGMA_CREAM);
        assert_eq!(entity_light(&player), 8);
        // Boots do not count.
        player.held = None;
        player.armor[0] = stack(89);
        assert_eq!(entity_light(&player), 0);
    }

    #[test]
    fn a_spectator_gives_no_light_and_a_dropped_item_gives_its_own() {
        let mut ghost = entity(EntityKind::Player);
        ghost.spectator = true;
        ghost.flags = EntityFlags::new(0x01, 0);
        ghost.held = stack(50);
        assert_eq!(entity_light(&ghost), 0);
        let mut drop = entity(EntityKind::Item);
        drop.item = stack(item::GLOWSTONE_DUST);
        assert_eq!(entity_light(&drop), 8);
        drop.item = stack(276);
        assert_eq!(entity_light(&drop), 0);
    }

    fn burning_pig(at: Vec3) -> EntityRenderSnapshot {
        let mut pig = EntityRenderSnapshot::new(EntityId(2), EntityKind::Pig, at);
        pig.flags = EntityFlags::new(0x01, 0);
        pig
    }

    #[test]
    fn off_gives_no_lights() {
        let mut collector = DynamicLightCollector::default();
        let pigs = [burning_pig(Vec3::ZERO)];
        let lights = collector.update(OffFastFancy::Off, Instant::now(), &pigs, DVec3::ZERO, false);
        assert!(lights.is_empty());
    }

    #[test]
    fn a_light_shines_from_the_eye_height_in_world_coordinates() {
        let mut collector = DynamicLightCollector::default();
        let mut pig = burning_pig(Vec3::new(1.0, 2.0, 3.0));
        pig.origin = DVec3::new(1_000_000.0, 60.0, 0.0);
        let lights = collector.update(
            OffFastFancy::Fancy,
            Instant::now(),
            &[pig],
            DVec3::ZERO,
            false,
        );
        let [light] = lights.as_slice() else {
            panic!("one light");
        };
        assert_eq!(light.level, 15);
        assert_eq!(light.position.x, 1_000_001.0);
        assert!((light.position.y - (62.0 + 0.9 * 0.85)).abs() < 1e-6);
        assert!(!light.underwater);
    }

    #[test]
    fn fancy_follows_every_tick_and_fast_every_half_second() {
        let start = Instant::now();
        let at = |x: f32| [burning_pig(Vec3::new(x, 0.0, 0.0))];
        let x_of =
            |collector: &mut DynamicLightCollector, mode: OffFastFancy, after_ms: u64, x: f32| {
                let lights = collector.update(
                    mode,
                    start + Duration::from_millis(after_ms),
                    &at(x),
                    DVec3::ZERO,
                    false,
                );
                lights.as_slice()[0].position.x
            };
        let mut fancy = DynamicLightCollector::default();
        assert_eq!(x_of(&mut fancy, OffFastFancy::Fancy, 0, 1.0), 1.0);
        assert_eq!(x_of(&mut fancy, OffFastFancy::Fancy, 20, 2.0), 1.0);
        assert_eq!(x_of(&mut fancy, OffFastFancy::Fancy, 50, 3.0), 3.0);
        let mut fast = DynamicLightCollector::default();
        assert_eq!(x_of(&mut fast, OffFastFancy::Fast, 0, 1.0), 1.0);
        assert_eq!(x_of(&mut fast, OffFastFancy::Fast, 300, 2.0), 1.0);
        assert_eq!(x_of(&mut fast, OffFastFancy::Fast, 500, 4.0), 4.0);
    }

    #[test]
    fn only_the_local_player_is_underwater_with_the_viewer() {
        let mut collector = DynamicLightCollector::default();
        let mut me = burning_pig(Vec3::ZERO);
        me.local = true;
        let other = burning_pig(Vec3::new(5.0, 0.0, 0.0));
        let lights = collector.update(
            OffFastFancy::Fancy,
            Instant::now(),
            &[me, other],
            DVec3::ZERO,
            true,
        );
        let wet: Vec<bool> = lights.as_slice().iter().map(|l| l.underwater).collect();
        assert_eq!(wet, [true, false]);
    }

    #[test]
    fn a_light_count_above_the_limit_keeps_the_nearest() {
        let mut collector = DynamicLightCollector::default();
        let pigs: Vec<_> = (0..40)
            .map(|index| burning_pig(Vec3::new(index as f32 * 3.0, 0.0, 0.0)))
            .collect();
        let lights = collector.update(
            OffFastFancy::Fancy,
            Instant::now(),
            &pigs,
            DVec3::ZERO,
            false,
        );
        assert_eq!(lights.as_slice().len(), umicraft_render::MAX_DYNAMIC_LIGHTS);
        assert!(lights
            .as_slice()
            .iter()
            .all(|light| light.position.x < 16.0 * 3.0));
    }
}
