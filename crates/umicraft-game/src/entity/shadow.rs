// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blob shadow under an entity (MCP `Render.renderShadow`).
//!
//! The renderer draws one textured square on the top of every full block in
//! the box `[x - size, x + size] x [y - size, y] x [z - size, z + size]` whose
//! cell above is lit (light above 3). Which blocks those are, and how bright,
//! only depends on the world, so the game thread finds them once per tick and
//! ships them in the frame; the opacity and the texture coordinates depend on
//! the interpolated position and the camera, and are the renderer's.

use std::sync::Arc;

use glam::Vec3;
use umicraft_world::{BlockPos, LightLevel, World};

use super::kind::EntityKind;
use super::meta::Variant;
use super::snapshot::EntityFrameData;
use crate::shapes::is_normal_cube;

/// One block a shadow falls on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowBlock {
    /// West and east edges of the block top.
    pub x: [f32; 2],
    /// North and south edges of the block top.
    pub z: [f32; 2],
    /// Height of the top of the block.
    pub top: f32,
    /// `World.getLightBrightness` of the cell above the block, `0..=1`.
    pub brightness: f32,
}

/// `World.calculateSkylightSubtracted` at `world_time`, without rain: how
/// much the sky light is reduced by the time of day.
#[must_use]
pub fn sky_subtracted(world_time: i64) -> u8 {
    // A negative time is a frozen clock; the cycle position is its magnitude.
    let ticks = (world_time.unsigned_abs() % 24_000) as f32;
    let mut angle = ticks / 24_000.0 - 0.25;
    if angle < 0.0 {
        angle += 1.0;
    }
    if angle > 1.0 {
        angle -= 1.0;
    }
    // `WorldProvider.calculateCelestialAngle`.
    let smoothed = 1.0
        - (f64::from(angle) * std::f64::consts::PI)
            .cos()
            .mul_add(0.5, 0.5) as f32;
    let celestial = angle + (smoothed - angle) / 3.0;
    // `calculateSkylightSubtracted`: `1 - (cos * 2 + 0.5)` kept to `0..=1`
    // (the rain and thunder factors are 1, then `1 - (1 - x)` cancels).
    let reduction = (1.0 - ((celestial * std::f32::consts::TAU).cos() * 2.0 + 0.5)).clamp(0.0, 1.0);
    (reduction * 11.0) as u8
}

/// `WorldProvider.lightBrightnessTable`.
fn light_brightness(level: u8) -> f32 {
    let missing = 1.0 - f32::from(level.min(15)) / 15.0;
    (1.0 - missing) / (missing * 3.0 + 1.0)
}

/// The size of the shadow of an entity (`Render.shadowSize` of its renderer,
/// halved for a baby and scaled by the size of a slime), `0` when it has none.
#[must_use]
pub fn shadow_size(kind: EntityKind, baby: bool, variant: &Variant) -> f32 {
    let base = match kind {
        EntityKind::Player
        | EntityKind::Zombie
        | EntityKind::ZombiePigman
        | EntityKind::ZombieVillager
        | EntityKind::Skeleton
        | EntityKind::Creeper
        | EntityKind::Enderman
        | EntityKind::Snowman
        | EntityKind::Witch
        | EntityKind::Villager
        | EntityKind::IronGolem
        | EntityKind::Wolf
        | EntityKind::Blaze
        | EntityKind::Ghast
        | EntityKind::Guardian
        | EntityKind::Dragon => 0.5,
        EntityKind::Cow | EntityKind::Mooshroom | EntityKind::Pig | EntityKind::Sheep => 0.7,
        EntityKind::Squid => 0.7,
        EntityKind::Horse => 0.75,
        EntityKind::Chicken
        | EntityKind::Rabbit
        | EntityKind::Silverfish
        | EntityKind::Endermite => 0.3,
        EntityKind::Ocelot => 0.4,
        EntityKind::Bat => 0.25,
        EntityKind::Spider | EntityKind::Wither => 1.0,
        EntityKind::CaveSpider => 0.7,
        EntityKind::Giant => 3.0,
        EntityKind::Item => 0.15,
        EntityKind::Slime | EntityKind::MagmaCube => {
            let size = match variant {
                Variant::Slime(size) => f32::from(*size),
                _ => 1.0,
            };
            0.25 * size
        }
        _ => return 0.0,
    };
    // `Render.renderShadow`: a baby living entity has half the shadow.
    if baby {
        base * 0.5
    } else {
        base
    }
}

/// The opacity factor of a shadow (`Render.shadowOpaque`): dropped items are
/// lighter than the rest.
#[must_use]
pub fn shadow_opacity(kind: EntityKind) -> f32 {
    if kind == EntityKind::Item {
        0.75
    } else {
        1.0
    }
}

/// The blocks the shadow of an entity of the given `size` at `position` falls
/// on. `full_cube` tells whether a block is a full cube and `light` gives the
/// light of a cell; both are the world's, taken as parameters so the rule can
/// be tested.
#[must_use]
pub fn shadow_blocks_with(
    position: Vec3,
    size: f32,
    sky_subtracted: u8,
    full_cube: impl Fn(BlockPos) -> bool,
    light: impl Fn(BlockPos) -> LightLevel,
) -> Vec<ShadowBlock> {
    let floor = |value: f32| value.floor() as i32;
    let (x0, x1) = (floor(position.x - size), floor(position.x + size));
    let (y0, y1) = (floor(position.y - size), floor(position.y));
    let (z0, z1) = (floor(position.z - size), floor(position.z + size));
    let mut blocks = Vec::new();
    for x in x0..=x1 {
        for y in y0..=y1 {
            for z in z0..=z1 {
                // The cell the entity stands in and the block under it.
                let cell = BlockPos::new(x, y, z);
                let below = BlockPos::new(x, y - 1, z);
                let level = light(cell);
                let lit = level.block.max(level.sky.saturating_sub(sky_subtracted));
                if lit <= 3 || !full_cube(below) {
                    continue;
                }
                blocks.push(ShadowBlock {
                    x: [x as f32, x as f32 + 1.0],
                    z: [z as f32, z as f32 + 1.0],
                    top: y as f32,
                    brightness: light_brightness(lit),
                });
            }
        }
    }
    blocks
}

/// Finds the shadow blocks of every entity in `entities` that casts a
/// shadow, from `world` at `world_time`.
pub fn shadow_entities(world: &World, entities: &mut [EntityFrameData], world_time: i64) {
    let subtracted = sky_subtracted(world_time);
    for entity in entities {
        let size = shadow_size(entity.kind, entity.meta.baby, &entity.meta.variant);
        if size <= 0.0 || entity.flags.is_invisible() {
            continue;
        }
        let blocks = shadow_blocks_with(
            entity.position,
            size,
            subtracted,
            |pos| is_normal_cube(world.block(pos)),
            |pos| world.light(pos),
        );
        entity.shadow = Arc::from(blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(top: i32) -> impl Fn(BlockPos) -> bool {
        move |pos| pos.y < top
    }

    fn day(_: BlockPos) -> LightLevel {
        LightLevel::FULL_SKY
    }

    #[test]
    fn the_clock_darkens_the_sky_light_at_night() {
        assert_eq!(sky_subtracted(6_000), 0, "noon");
        assert!(sky_subtracted(18_000) >= 10, "midnight");
        assert!(sky_subtracted(13_000) > sky_subtracted(12_000), "dusk");
        assert_eq!(
            sky_subtracted(-6_000),
            sky_subtracted(6_000),
            "a frozen clock"
        );
    }

    #[test]
    fn the_light_curve_matches_the_reference_table() {
        assert_eq!(light_brightness(0), 0.0);
        assert_eq!(light_brightness(15), 1.0);
        // `(1 - f) / (3 f + 1)` with `f = 1 - 8 / 15`.
        assert!((light_brightness(8) - 0.2222).abs() < 1e-3);
    }

    #[test]
    fn a_pig_standing_on_a_block_casts_a_shadow_on_it() {
        let blocks = shadow_blocks_with(Vec3::new(0.5, 64.0, 0.5), 0.7, 0, ground(64), day);
        assert!(!blocks.is_empty());
        // The ground is the layer under the feet; the box 0.7 wide reaches the
        // blocks around the one it stands on.
        // The blocks under the feet, and the layer under it that the first
        // layer hides.
        assert!(blocks.iter().any(|block| block.top == 64.0));
        assert!(blocks.iter().all(|block| block.top <= 64.0));
        let columns: std::collections::BTreeSet<i32> =
            blocks.iter().map(|block| block.x[0] as i32).collect();
        assert_eq!(columns, [-1, 0, 1].into_iter().collect());
    }

    #[test]
    fn no_shadow_without_ground_or_without_light() {
        let hovering = shadow_blocks_with(Vec3::new(0.5, 80.0, 0.5), 0.5, 0, ground(64), day);
        assert!(hovering.is_empty(), "the ground is more than the size away");
        let dark = shadow_blocks_with(Vec3::new(0.5, 64.0, 0.5), 0.5, 0, ground(64), |_| {
            LightLevel { block: 3, sky: 0 }
        });
        assert!(dark.is_empty(), "light 3 is not above 3");
        // At night the sky light is reduced by 11: 15 still leaves 4, which is
        // above 3 but dim, and 14 leaves exactly 3.
        let night = shadow_blocks_with(Vec3::new(0.5, 64.0, 0.5), 0.5, 11, ground(64), day);
        assert!(!night.is_empty());
        let noon = shadow_blocks_with(Vec3::new(0.5, 64.0, 0.5), 0.5, 0, ground(64), day);
        assert!(night[0].brightness < noon[0].brightness);
        let moonless = shadow_blocks_with(Vec3::new(0.5, 64.0, 0.5), 0.5, 11, ground(64), |_| {
            LightLevel { block: 0, sky: 14 }
        });
        assert!(moonless.is_empty());
        let torch = shadow_blocks_with(Vec3::new(0.5, 64.0, 0.5), 0.5, 11, ground(64), |_| {
            LightLevel { block: 10, sky: 15 }
        });
        assert!(!torch.is_empty());
    }

    #[test]
    fn the_sizes_follow_the_renderers() {
        let none = Variant::None;
        assert_eq!(shadow_size(EntityKind::Player, false, &none), 0.5);
        assert_eq!(shadow_size(EntityKind::Pig, false, &none), 0.7);
        assert_eq!(shadow_size(EntityKind::Pig, true, &none), 0.35);
        assert_eq!(shadow_size(EntityKind::Giant, false, &none), 3.0);
        assert_eq!(
            shadow_size(EntityKind::Slime, false, &Variant::Slime(4)),
            1.0
        );
        assert_eq!(shadow_size(EntityKind::Arrow, false, &none), 0.0);
        assert_eq!(shadow_opacity(EntityKind::Item), 0.75);
    }
}
