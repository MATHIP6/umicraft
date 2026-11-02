// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bounding box of an entity, as the crosshair ray sees it.
//!
//! MCP `EntityRenderer.getMouseOver` tests the ray against
//! `Entity.getEntityBoundingBox()` grown by `getCollisionBorderSize()` (0.1),
//! and only for entities whose `canBeCollidedWith()` is true: living
//! entities, hanging entities, boats, minecarts, end crystals, fireballs and
//! armour stands that are not markers. Dropped items, arrows, bobbers and
//! the dragon body (its parts are not tracked) are never targeted.

use glam::DVec3;

use crate::entity::meta::Variant;
use crate::entity::painting::ARTS;
use crate::entity::{EntityKind, EntityRecord};
use crate::shapes::Aabb;

/// The growth of every targeted box (MCP `Entity.getCollisionBorderSize`).
const COLLISION_BORDER: f64 = 0.1;

/// Size of a mob that is a child (MCP `EntityAgeable.setScaleForAge`).
const CHILD_SCALE: f64 = 0.5;

/// Object type ids of the collidable objects that have no [`EntityKind`] of
/// their own (MCP `EntityList` object ids).
const BOAT: u8 = 1;
const MINECART: u8 = 10;
const END_CRYSTAL: u8 = 51;
const LARGE_FIREBALL: u8 = 63;
const SMALL_FIREBALL: u8 = 64;
const WITHER_SKULL: u8 = 66;

/// Armour stand size (MCP `EntityArmorStand`, `setSize(0.5F, 1.975F)`).
const ARMOR_STAND: (f64, f64) = (0.5, 1.975);

/// Item frame half sizes: 12 pixels wide and tall, one pixel deep
/// (MCP `EntityHanging.updateBoundingBox`, `/ 32`).
const FRAME_HALF_FACE: f64 = 12.0 / 32.0;
const FRAME_HALF_DEPTH: f64 = 1.0 / 32.0;
/// How far the centre of a frame sits from the middle of its block.
const FRAME_WALL_OFFSET: f64 = 0.46875;

/// The box the ray tests for `entity`, `None` when the ray ignores it.
///
/// The box already carries the collision border.
#[must_use]
pub fn hitbox(entity: &EntityRecord) -> Option<Aabb> {
    match entity.kind {
        EntityKind::ItemFrame => return Some(frame_box(entity)),
        EntityKind::Painting => return painting_box(entity),
        EntityKind::LeashKnot => return Some(knot_box(entity)),
        _ => {}
    }
    let (width, height) = size(entity)?;
    let half = width / 2.0 + COLLISION_BORDER;
    let feet = entity.position;
    Some(Aabb::new(
        DVec3::new(feet.x - half, feet.y - COLLISION_BORDER, feet.z - half),
        DVec3::new(
            feet.x + half,
            feet.y + height + COLLISION_BORDER,
            feet.z + half,
        ),
    ))
}

/// The width and height of an entity that can be targeted, without the
/// collision border (MCP `Entity.width`, `Entity.height`).
#[must_use]
pub fn body_size(entity: &EntityRecord) -> Option<(f64, f64)> {
    size(entity)
}

/// The width and height of an entity that can be targeted.
fn size(entity: &EntityRecord) -> Option<(f64, f64)> {
    match entity.kind {
        EntityKind::Item | EntityKind::Arrow | EntityKind::FishHook | EntityKind::Dragon => None,
        EntityKind::ArmorStand => match entity.meta.variant {
            Variant::ArmorStand(stand) if stand.flags & 16 != 0 => None,
            _ => Some(ARMOR_STAND),
        },
        EntityKind::Slime | EntityKind::MagmaCube => {
            let side = match entity.meta.variant {
                Variant::Slime(size) => 0.6 * f64::from(size),
                _ => 0.6,
            };
            Some((side, side))
        }
        EntityKind::Other(id) => object_size(id),
        // The objects with a body (MCP `canBeCollidedWith`, sizes of their
        // constructors).
        EntityKind::Boat => Some((1.5, 0.6)),
        EntityKind::Minecart => Some((0.98, 0.7)),
        EntityKind::EnderCrystal => Some((2.0, 2.0)),
        EntityKind::LargeFireball => Some((1.0, 1.0)),
        EntityKind::PrimedTnt | EntityKind::FallingBlock => Some((0.98, 0.98)),
        // Everything else the ray passes through: small fireballs and wither
        // skulls answer false, thrown objects, rockets, orbs and bolts never
        // collide.
        EntityKind::SmallFireball
        | EntityKind::WitherSkull
        | EntityKind::Snowball
        | EntityKind::Egg
        | EntityKind::EnderPearl
        | EntityKind::EyeOfEnder
        | EntityKind::Potion
        | EntityKind::ExpBottle
        | EntityKind::FireworkRocket
        | EntityKind::XpOrb
        | EntityKind::Lightning => None,
        kind => {
            let scale = if entity.meta.baby { CHILD_SCALE } else { 1.0 };
            Some((
                f64::from(kind.width()) * scale,
                f64::from(kind.height()) * scale,
            ))
        }
    }
}

/// The size of the collidable objects (MCP `EntityBoat`, `EntityMinecart`,
/// `EntityEnderCrystal`, `EntityFireball` constructors).
fn object_size(object_type: u8) -> Option<(f64, f64)> {
    match object_type {
        BOAT => Some((1.5, 0.6)),
        MINECART => Some((0.98, 0.7)),
        END_CRYSTAL => Some((2.0, 2.0)),
        LARGE_FIREBALL => Some((1.0, 1.0)),
        SMALL_FIREBALL | WITHER_SKULL => Some((0.3125, 0.3125)),
        _ => None,
    }
}

/// The box of an item frame, thin along the wall direction (MCP
/// `EntityHanging.updateBoundingBox`).
///
/// The position of a frame is its block (`Spawn Object` sends the hanging
/// position); the box is centred on the block, pushed 0.46875 against the
/// wall it hangs on.
fn frame_box(entity: &EntityRecord) -> Aabb {
    // Directions 0 (south), 1 (west), 2 (north), 3 (east): the front offset.
    let (front_x, front_z) = match entity.frame_direction % 4 {
        0 => (0.0, 1.0),
        1 => (-1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (1.0, 0.0),
    };
    let block = entity.position.floor();
    let centre = DVec3::new(
        block.x + 0.5 - front_x * FRAME_WALL_OFFSET,
        block.y + 0.5,
        block.z + 0.5 - front_z * FRAME_WALL_OFFSET,
    );
    let (half_x, half_z) = if front_z != 0.0 {
        (FRAME_HALF_FACE, FRAME_HALF_DEPTH)
    } else {
        (FRAME_HALF_DEPTH, FRAME_HALF_FACE)
    };
    let half = DVec3::new(half_x, FRAME_HALF_FACE, half_z) + DVec3::splat(COLLISION_BORDER);
    Aabb::new(centre - half, centre + half)
}

/// The box of a painting (MCP `EntityHanging.updateBoundingBox`): centred on
/// the record, `width / 32` either side along the wall, `1 / 32` deep and
/// `height / 32` either side up. `None` for an artwork the table does not know.
fn painting_box(entity: &EntityRecord) -> Option<Aabb> {
    let art = ARTS.get(usize::try_from(entity.object_data).ok()?)?;
    let facing = ((entity.yaw / 90.0).round() as i32).rem_euclid(4);
    let along = f64::from(art.width) / 32.0;
    // South and north (0 and 2) run along x, west and east along z.
    let (half_x, half_z) = if facing % 2 == 0 {
        (along, 1.0 / 32.0)
    } else {
        (1.0 / 32.0, along)
    };
    let half =
        DVec3::new(half_x, f64::from(art.height) / 32.0, half_z) + DVec3::splat(COLLISION_BORDER);
    Some(Aabb::new(entity.position - half, entity.position + half))
}

/// The box of a leash knot (MCP `EntityLeashKnot.updateBoundingBox`): 0.375
/// wide, from an eighth of a block under its centre to three eighths over.
fn knot_box(entity: &EntityRecord) -> Aabb {
    let centre = entity.position;
    let half = DVec3::new(0.1875, 0.0, 0.1875) + DVec3::splat(COLLISION_BORDER);
    Aabb::new(
        DVec3::new(
            centre.x - half.x,
            centre.y - 0.125 - COLLISION_BORDER,
            centre.z - half.z,
        ),
        DVec3::new(
            centre.x + half.x,
            centre.y + 0.375 + COLLISION_BORDER,
            centre.z + half.z,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityId;

    fn record(kind: EntityKind, position: DVec3) -> EntityRecord {
        EntityRecord::new(EntityId(1), kind, position, 0.0, 0.0)
    }

    #[test]
    fn a_zombie_box_is_its_size_plus_the_border() {
        let zombie = record(EntityKind::Zombie, DVec3::new(10.0, 64.0, 10.0));
        let bounds = hitbox(&zombie).unwrap();
        assert!((bounds.min.x - 9.6).abs() < 1e-5, "0.3 + 0.1 either side");
        assert!((bounds.max.y - (64.0 + 1.95 + 0.1)).abs() < 1e-5);
        assert!((bounds.min.y - 63.9).abs() < 1e-5);
    }

    #[test]
    fn items_arrows_and_bobbers_are_never_targeted() {
        for kind in [EntityKind::Item, EntityKind::Arrow, EntityKind::FishHook] {
            assert!(hitbox(&record(kind, DVec3::ZERO)).is_none());
        }
    }

    #[test]
    fn a_marker_armour_stand_cannot_be_targeted() {
        let mut stand = record(EntityKind::ArmorStand, DVec3::ZERO);
        assert!(hitbox(&stand).is_some());
        stand.meta.variant = Variant::ArmorStand(crate::entity::meta::ArmorStandMeta {
            flags: 16,
            ..Default::default()
        });
        assert!(hitbox(&stand).is_none());
    }

    #[test]
    fn a_child_is_half_the_size() {
        let mut pig = record(EntityKind::Pig, DVec3::ZERO);
        let adult = hitbox(&pig).unwrap();
        pig.meta.baby = true;
        let child = hitbox(&pig).unwrap();
        assert!(child.max.y < adult.max.y);
        assert!((child.max.y - (0.45 + 0.1)).abs() < 1e-5);
    }

    #[test]
    fn a_slime_grows_with_its_size() {
        let mut slime = record(EntityKind::Slime, DVec3::ZERO);
        slime.meta.variant = Variant::Slime(4);
        let bounds = hitbox(&slime).unwrap();
        assert!((bounds.max.y - (2.4 + 0.1)).abs() < 1e-5);
    }

    #[test]
    fn an_item_frame_is_thin_along_its_wall_direction() {
        let mut frame = record(EntityKind::ItemFrame, DVec3::new(0.0, 65.0, 0.0));
        frame.frame_direction = 0;
        let south = hitbox(&frame).unwrap();
        assert!(south.max.x - south.min.x > south.max.z - south.min.z);
        frame.frame_direction = 1;
        let west = hitbox(&frame).unwrap();
        assert!(west.max.z - west.min.z > west.max.x - west.min.x);
        assert!(
            west.min.x > 0.8,
            "a west-facing frame hangs on the east wall"
        );
    }

    #[test]
    fn boats_and_minecarts_are_targeted_and_other_objects_are_not() {
        assert!(hitbox(&record(EntityKind::Other(1), DVec3::ZERO)).is_some());
        assert!(hitbox(&record(EntityKind::Other(10), DVec3::ZERO)).is_some());
        assert!(hitbox(&record(EntityKind::Other(61), DVec3::ZERO)).is_none());
    }

    #[test]
    fn a_boat_a_cart_a_crystal_and_tnt_have_their_vanilla_boxes() {
        let width = |kind| {
            let bounds = hitbox(&record(kind, DVec3::ZERO)).unwrap();
            (
                bounds.max.x - bounds.min.x - 0.2,
                bounds.max.y - bounds.min.y - 0.2,
            )
        };
        let close =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
        assert!(close(width(EntityKind::Boat), (1.5, 0.6)));
        assert!(close(width(EntityKind::Minecart), (0.98, 0.7)));
        assert!(close(width(EntityKind::EnderCrystal), (2.0, 2.0)));
        assert!(close(width(EntityKind::PrimedTnt), (0.98, 0.98)));
    }

    #[test]
    fn thrown_objects_and_small_fireballs_are_not_targeted() {
        for kind in [
            EntityKind::Snowball,
            EntityKind::EnderPearl,
            EntityKind::SmallFireball,
            EntityKind::WitherSkull,
            EntityKind::XpOrb,
            EntityKind::Lightning,
        ] {
            assert!(hitbox(&record(kind, DVec3::ZERO)).is_none(), "{kind:?}");
        }
    }

    #[test]
    fn a_painting_is_a_thin_slab_as_wide_as_its_artwork() {
        let mut painting = record(EntityKind::Painting, DVec3::new(0.0, 65.0, 0.5));
        // The first artwork is 16 by 16 pixels, hung facing south.
        painting.object_data = 0;
        let south = hitbox(&painting).unwrap();
        assert!((south.max.x - south.min.x - (1.0 + 0.2)).abs() < 1e-9);
        assert!((south.max.z - south.min.z - (1.0 / 16.0 + 0.2)).abs() < 1e-9);
        painting.yaw = 90.0;
        let west = hitbox(&painting).unwrap();
        assert!(west.max.x - west.min.x < west.max.z - west.min.z);
        painting.object_data = 9999;
        assert!(hitbox(&painting).is_none());
    }

    #[test]
    fn a_leash_knot_is_a_small_box_above_the_middle_of_its_fence() {
        let knot = record(EntityKind::LeashKnot, DVec3::new(3.5, 64.5, 3.5));
        let bounds = hitbox(&knot).unwrap();
        assert!((bounds.max.x - bounds.min.x - (0.375 + 0.2)).abs() < 1e-9);
        assert!((bounds.min.y - (64.5 - 0.125 - 0.1)).abs() < 1e-9);
        assert!((bounds.max.y - (64.5 + 0.375 + 0.1)).abs() < 1e-9);
    }
}
