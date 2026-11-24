// SPDX-License-Identifier: MIT OR Apache-2.0

//! Water and lava on the objects the client simulates.
//!
//! In the reference every entity runs `handleWaterMovement` at the start of
//! `Entity.onUpdate`, so the current of flowing water pushes it by 0.014 per
//! tick (`World.handleMaterialAcceleration`); the projectiles then read the
//! `inWater` flag it sets to pick a stronger drag (`EntityArrow` 0.6,
//! `EntityThrowable` and `EntityFireball` 0.8). A dropped item overrides the
//! method to scan its whole box, and a primed TNT calls it again after its
//! move. A dropped item in lava is thrown up and sideways (`EntityItem`).

use glam::DVec3;
use umicraft_world::fluid::Liquid;

use super::objects::bounds;
use super::record::EntityRecord;
use crate::media::water_current;
use crate::shapes::BlockSource;

/// How the box of an entity is scanned for water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scan {
    /// `Entity.handleWaterMovement`: the box shrunk by 0.4 vertically and
    /// 0.001 on every side.
    Shrunk,
    /// `EntityItem.handleWaterMovement`: the whole box.
    Whole,
}

/// The vertical speed a dropped item is thrown at out of lava
/// (`EntityItem.onUpdate`, `0.2`).
const LAVA_KICK: f32 = 0.2;

/// `handleWaterMovement`: pushes `entity` along the current of the water it
/// stands in; `true` when it is in water.
pub(super) fn handle_water<S: BlockSource + ?Sized>(
    entity: &mut EntityRecord,
    world: &S,
    scan: Scan,
) -> bool {
    let bounds = bounds(entity);
    let shrink = match scan {
        Scan::Shrunk => DVec3::new(0.001, 0.4 + 0.001, 0.001),
        Scan::Whole => DVec3::ZERO,
    };
    match water_current(bounds.min + shrink, bounds.max - shrink, world) {
        Some(push) => {
            entity.velocity += push.as_vec3();
            true
        }
        None => false,
    }
}

/// The lava check of `EntityItem.onUpdate`: when the item changed block (the
/// reference compares the truncated coordinates) or every 25th tick, an item
/// whose block is lava jumps with a random sideways push.
///
/// The reference rolls `rand`, which the server never sees, so the push comes
/// from a hash of the entity and its age instead.
pub(super) fn kick_out_of_lava<S: BlockSource + ?Sized>(
    entity: &mut EntityRecord,
    before: DVec3,
    world: &S,
) {
    let moved = before.as_ivec3() != entity.position.as_ivec3();
    if !moved && !entity.age_ticks.is_multiple_of(25) {
        return;
    }
    let at = entity.position.floor().as_ivec3();
    if Liquid::Lava.contains(world.block(at.x, at.y, at.z)) {
        let seed = u64::from(entity.id.0 as u32) << 32 | u64::from(entity.age_ticks);
        entity.velocity.y = LAVA_KICK;
        entity.velocity.x = (unit(seed) - unit(seed ^ 1)) * 0.2;
        entity.velocity.z = (unit(seed ^ 2) - unit(seed ^ 3)) * 0.2;
    }
}

/// A float in `0..1` hashed from `seed` (SplitMix64), standing in for
/// `Random.nextFloat`.
fn unit(seed: u64) -> f32 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::kind::{EntityId, EntityKind};
    use umicraft_world::BlockState;

    /// Water (id 9) up to `surface`, with `lava` instead when set.
    struct Pool {
        surface: i32,
        liquid: u16,
    }

    impl BlockSource for Pool {
        fn block(&self, _x: i32, y: i32, _z: i32) -> BlockState {
            if y < 60 {
                BlockState::new(1, 0)
            } else if y < self.surface {
                BlockState::new(self.liquid, 0)
            } else {
                BlockState::AIR
            }
        }
    }

    const WATER: Pool = Pool {
        surface: 64,
        liquid: 9,
    };

    fn at(kind: EntityKind, y: f64) -> EntityRecord {
        EntityRecord::new(EntityId(7), kind, DVec3::new(0.5, y, 0.5), 0.0, 0.0)
    }

    #[test]
    fn an_object_in_still_water_is_wet_and_not_pushed() {
        let mut arrow = at(EntityKind::Arrow, 62.0);
        assert!(handle_water(&mut arrow, &WATER, Scan::Shrunk));
        assert_eq!(arrow.velocity, glam::Vec3::ZERO);
    }

    #[test]
    fn an_object_in_the_air_is_dry() {
        let mut arrow = at(EntityKind::Arrow, 70.0);
        assert!(!handle_water(&mut arrow, &WATER, Scan::Shrunk));
    }

    #[test]
    fn a_dropped_item_scans_its_whole_box() {
        // The item rests on the floor of a pool one block deep: its box
        // shrunk by 0.4 is above the surface test, the whole box is not.
        let shallow = Pool {
            surface: 61,
            liquid: 9,
        };
        let mut item = at(EntityKind::Item, 60.0);
        assert!(handle_water(&mut item, &shallow, Scan::Whole));
        assert!(!handle_water(&mut item, &shallow, Scan::Shrunk));
    }

    #[test]
    fn a_dropped_item_in_lava_jumps_and_the_jump_repeats_every_25_ticks() {
        let lava = Pool {
            surface: 64,
            liquid: 11,
        };
        let mut item = at(EntityKind::Item, 62.0);
        item.age_ticks = 25;
        let start = item.position;
        kick_out_of_lava(&mut item, start, &lava);
        assert_eq!(item.velocity.y, LAVA_KICK);
        assert!(item.velocity.x.abs() <= 0.2 && item.velocity.z.abs() <= 0.2);
        // Between two checks, in the same block: nothing.
        let mut idle = at(EntityKind::Item, 62.0);
        idle.age_ticks = 13;
        let start = idle.position;
        kick_out_of_lava(&mut idle, start, &lava);
        assert_eq!(idle.velocity, glam::Vec3::ZERO);
    }

    use super::super::objects::{step, Fate};
    use glam::Vec3;

    /// Flies `kind` for one tick from `velocity`, in water or in the air.
    fn after_one_tick(kind: EntityKind, velocity: Vec3, in_water: bool) -> Vec3 {
        // 62.3: a box 0.25 tall shrunk by 0.4 is inverted, and the scan of the
        // reference finds water only for some heights (0.15 to 0.6 of a block).
        let mut object = at(kind, 62.3);
        object.velocity = velocity;
        let (wet, dry) = (
            &WATER,
            &Pool {
                surface: 60,
                liquid: 9,
            },
        );
        assert_eq!(
            step(&mut object, if in_water { wet } else { dry }, &[]),
            Fate::Stays
        );
        object.velocity
    }

    #[test]
    fn an_arrow_in_water_keeps_0_6_of_its_speed() {
        // `EntityArrow.onUpdate`: `f4 = 0.6`, then 0.05 of gravity.
        let v = after_one_tick(EntityKind::Arrow, Vec3::new(0.0, 0.0, 1.0), true);
        assert!((v.z - 0.6).abs() < 1e-6 && (v.y + 0.05).abs() < 1e-6, "{v}");
        let v = after_one_tick(EntityKind::Arrow, Vec3::new(0.0, 0.0, 1.0), false);
        assert!((v.z - 0.99).abs() < 1e-6, "{v}");
    }

    #[test]
    fn a_thrown_object_in_water_keeps_0_8_of_its_speed() {
        // `EntityThrowable.onUpdate`: `f2 = 0.8`, then the gravity of the class.
        let v = after_one_tick(EntityKind::Snowball, Vec3::new(0.0, 0.0, 1.0), true);
        assert!((v.z - 0.8).abs() < 1e-6 && (v.y + 0.03).abs() < 1e-6, "{v}");
    }

    #[test]
    fn a_fireball_in_water_keeps_0_8_instead_of_0_95() {
        // `EntityFireball.onUpdate`: `f2 = 0.8` in water (0.95 otherwise).
        let v = after_one_tick(EntityKind::LargeFireball, Vec3::new(0.0, 0.0, 1.0), true);
        assert!((v.z - 1.1 * 0.8).abs() < 1e-5, "{v}");
        let v = after_one_tick(EntityKind::LargeFireball, Vec3::new(0.0, 0.0, 1.0), false);
        assert!((v.z - 1.1 * 0.95).abs() < 1e-5, "{v}");
    }

    #[test]
    fn flowing_water_pushes_a_falling_block_downstream() {
        // Level 1 water with level 2 to the east flows east (`getFlowVector`).
        struct Stream;
        impl BlockSource for Stream {
            fn block(&self, x: i32, y: i32, _z: i32) -> BlockState {
                match (x, y) {
                    (0, 62) => BlockState::new(8, 1),
                    (1, 62) => BlockState::new(8, 2),
                    (_, 62) => BlockState::new(1, 0),
                    _ => BlockState::AIR,
                }
            }
        }
        let mut block = at(EntityKind::FallingBlock, 62.0);
        step(&mut block, &Stream, &[]);
        assert!(
            (block.velocity.x - 0.014 * 0.98).abs() < 1e-6,
            "{}",
            block.velocity
        );
    }

    #[test]
    fn the_hash_is_a_float_in_the_unit_interval() {
        for seed in 0..1000 {
            assert!((0.0..1.0).contains(&unit(seed)));
        }
    }
}
