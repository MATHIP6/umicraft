// SPDX-License-Identifier: MIT OR Apache-2.0

//! What an arrow in flight does when it meets another entity on the client.
//!
//! `EntityArrow.onUpdate` tests the entities on the way without checking
//! `isRemote`, so the client runs the same hit as the server, but the damage
//! is applied by `attackEntityFrom`, which answers `false` on a client for
//! every living entity (`EntityLivingBase`, `EntityPlayer`, the armour stand),
//! for a primed TNT and for a falling block. The arrow then bounces: its
//! motion is multiplied by -0.1 and `ticksInAir` restarts. Boats, minecarts,
//! ender crystals and hanging entities answer `true`, so the arrow is
//! destroyed (`setDead`) unless the target is an enderman. Snowballs, eggs,
//! pearls and fireballs test entities on the server only (`!isRemote`) or do
//! nothing with the hit on the client, so they fly through.

use glam::DVec3;

use super::kind::{EntityId, EntityKind};
use super::record::EntityRecord;
use crate::collision::Aabb;

/// How far a target box grows for the test (`EntityArrow`, `f1 = 0.3`).
const HIT_MARGIN: f64 = 0.3;

/// Ticks before an arrow can hit the entity that shot it.
const SHOOTER_GRACE_TICKS: u32 = 5;

/// What the arrow does after a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    /// `attackEntityFrom` said no: the arrow rebounds slowly.
    Bounce,
    /// `attackEntityFrom` said yes: the arrow is destroyed.
    Absorb,
}

/// An entity an arrow can hit, with its box at the start of the tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Target {
    id: EntityId,
    hit: Hit,
    bounds: Aabb,
}

/// What `canBeCollidedWith` and the client answer of `attackEntityFrom` make
/// of `kind`, or `None` when arrows pass through it.
const fn reaction(kind: EntityKind) -> Option<Hit> {
    match kind {
        EntityKind::Boat | EntityKind::Minecart | EntityKind::EnderCrystal => Some(Hit::Absorb),
        EntityKind::PrimedTnt | EntityKind::FallingBlock => Some(Hit::Bounce),
        kind if kind.is_living() => Some(Hit::Bounce),
        _ => None,
    }
}

/// The entities an arrow can hit, boxed from their feet.
///
/// The reference first gathers the entities near the swept box of the arrow;
/// that margin (1 block) always exceeds the 0.3 of the test, so the filter
/// would never exclude a hit and is left out.
pub(super) fn targets<'a>(entities: impl Iterator<Item = &'a EntityRecord>) -> Vec<Target> {
    entities
        .filter_map(|entity| {
            let hit = reaction(entity.kind)?;
            let half = f64::from(entity.kind.width()) / 2.0;
            let height = f64::from(entity.kind.height());
            let feet = entity.position;
            let bounds = Aabb::new(
                DVec3::new(feet.x - half, feet.y, feet.z - half)
                    - DVec3::new(HIT_MARGIN, HIT_MARGIN, HIT_MARGIN),
                DVec3::new(feet.x + half, feet.y + height, feet.z + half)
                    + DVec3::new(HIT_MARGIN, HIT_MARGIN, HIT_MARGIN),
            );
            Some(Target {
                id: entity.id,
                hit,
                bounds,
            })
        })
        .collect()
}

/// The nearest target the segment `from..to` of `arrow` crosses.
///
/// `to` is the end of the flight, already cut at the first block. The shooter
/// is skipped for its first ticks in the air (`ticksInAir >= 5`).
pub(super) fn first_hit(
    arrow: &EntityRecord,
    from: DVec3,
    to: DVec3,
    targets: &[Target],
) -> Option<Hit> {
    targets
        .iter()
        .filter(|target| target.id.0 != arrow.object_data || arrow.air_ticks >= SHOOTER_GRACE_TICKS)
        .filter_map(|target| {
            intercept(target.bounds, from, to).map(|distance| (distance, target.hit))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, hit)| hit)
}

/// `AxisAlignedBB.calculateIntercept`: the squared distance from `from` to
/// the nearest point where the segment meets a face of `bounds`, if it does.
/// A segment that starts inside meets the face it leaves by, as in the
/// reference.
fn intercept(bounds: Aabb, from: DVec3, to: DVec3) -> Option<f64> {
    let mut nearest: Option<f64> = None;
    for axis in 0..3 {
        for plane in [bounds.min[axis], bounds.max[axis]] {
            let Some(point) = at_plane(from, to, axis, plane) else {
                continue;
            };
            let inside = (0..3).filter(|&other| other != axis).all(|other| {
                point[other] >= bounds.min[other] && point[other] <= bounds.max[other]
            });
            if inside {
                let distance = from.distance_squared(point);
                nearest = Some(nearest.map_or(distance, |best| best.min(distance)));
            }
        }
    }
    nearest
}

/// `Vec3.getIntermediateWithXValue` and its siblings: where the segment
/// crosses the plane `axis = value`, or `None` when it runs parallel to it
/// (the reference rejects a squared difference under 1e-7) or stops short.
fn at_plane(from: DVec3, to: DVec3, axis: usize, value: f64) -> Option<DVec3> {
    let delta = to - from;
    if delta[axis] * delta[axis] < 1.0e-7 {
        return None;
    }
    let t = (value - from[axis]) / delta[axis];
    (0.0..=1.0).contains(&t).then(|| from + delta * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: i32, kind: EntityKind, at: DVec3) -> EntityRecord {
        EntityRecord::new(EntityId(id), kind, at, 0.0, 0.0)
    }

    fn arrow_at(at: DVec3, shooter: i32) -> EntityRecord {
        let mut arrow = entity(1, EntityKind::Arrow, at);
        arrow.object_data = shooter;
        arrow
    }

    fn hit(arrow: &EntityRecord, to: DVec3, others: &[EntityRecord]) -> Option<Hit> {
        let targets = targets(others.iter());
        first_hit(arrow, arrow.position, to, &targets)
    }

    #[test]
    fn an_arrow_bounces_off_a_mob_in_its_way() {
        let cow = entity(2, EntityKind::Cow, DVec3::new(0.0, 64.0, 5.0));
        let arrow = arrow_at(DVec3::new(0.0, 64.5, 0.0), 0);
        assert_eq!(
            hit(&arrow, DVec3::new(0.0, 64.5, 8.0), &[cow]),
            Some(Hit::Bounce)
        );
    }

    #[test]
    fn the_box_grows_by_0_3() {
        let cow = entity(2, EntityKind::Cow, DVec3::new(0.0, 64.0, 5.0));
        // 0.4 + 0.3 = 0.7 beside the middle of a cow 0.9 wide: just inside.
        let near = arrow_at(DVec3::new(0.74, 64.5, 0.0), 0);
        assert!(hit(
            &near,
            DVec3::new(0.74, 64.5, 8.0),
            std::slice::from_ref(&cow)
        )
        .is_some());
        let far = arrow_at(DVec3::new(0.76, 64.5, 0.0), 0);
        assert!(hit(&far, DVec3::new(0.76, 64.5, 8.0), &[cow]).is_none());
    }

    #[test]
    fn a_target_beyond_the_end_of_the_flight_is_not_hit() {
        let cow = entity(2, EntityKind::Cow, DVec3::new(0.0, 64.0, 5.0));
        let arrow = arrow_at(DVec3::new(0.0, 64.5, 0.0), 0);
        assert_eq!(hit(&arrow, DVec3::new(0.0, 64.5, 3.0), &[cow]), None);
    }

    #[test]
    fn the_nearest_of_two_targets_decides() {
        let boat = entity(2, EntityKind::Boat, DVec3::new(0.0, 64.0, 4.0));
        let cow = entity(3, EntityKind::Cow, DVec3::new(0.0, 64.0, 6.0));
        let arrow = arrow_at(DVec3::new(0.0, 64.2, 0.0), 0);
        let end = DVec3::new(0.0, 64.2, 9.0);
        assert_eq!(
            hit(&arrow, end, &[cow.clone(), boat.clone()]),
            Some(Hit::Absorb)
        );
        assert_eq!(hit(&arrow, end, &[boat]), Some(Hit::Absorb));
        assert_eq!(hit(&arrow, end, &[cow]), Some(Hit::Bounce));
    }

    #[test]
    fn the_shooter_is_safe_for_five_ticks() {
        let archer = entity(9, EntityKind::Player, DVec3::new(0.0, 64.0, 0.0));
        let mut arrow = arrow_at(DVec3::new(0.0, 64.5, -1.0), 9);
        let end = DVec3::new(0.0, 64.5, 2.0);
        assert_eq!(hit(&arrow, end, std::slice::from_ref(&archer)), None);
        arrow.air_ticks = 5;
        assert_eq!(hit(&arrow, end, &[archer]), Some(Hit::Bounce));
    }

    #[test]
    fn items_and_other_arrows_are_passed_through() {
        let item = entity(2, EntityKind::Item, DVec3::new(0.0, 64.0, 5.0));
        let other = entity(3, EntityKind::Arrow, DVec3::new(0.0, 64.0, 5.0));
        let arrow = arrow_at(DVec3::new(0.0, 64.5, 0.0), 0);
        assert_eq!(
            hit(&arrow, DVec3::new(0.0, 64.5, 8.0), &[item, other]),
            None
        );
    }

    use super::super::objects::{arrow_targets, step, Fate};
    use crate::shapes::BlockSource;
    use glam::Vec3;
    use umicraft_world::BlockState;

    struct Air;
    impl BlockSource for Air {
        fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            BlockState::AIR
        }
    }

    fn fly_into(target: EntityRecord) -> (EntityRecord, Fate) {
        let mut arrow = arrow_at(DVec3::new(0.0, 64.5, 0.0), 0);
        arrow.velocity = Vec3::new(0.0, 0.0, 2.0);
        arrow.air_ticks = 9;
        let targets = arrow_targets([&arrow, &target].into_iter());
        let fate = step(&mut arrow, &Air, &targets);
        (arrow, fate)
    }

    #[test]
    fn an_arrow_that_hits_a_mob_bounces_back_slowly() {
        // `attackEntityFrom` is false on a client: motion * -0.1, ticksInAir 0.
        let (arrow, fate) = fly_into(entity(2, EntityKind::Pig, DVec3::new(0.0, 64.0, 1.5)));
        assert_eq!(fate, Fate::Stays);
        assert_eq!(arrow.air_ticks, 0);
        assert!(
            (arrow.velocity.z + 0.2 * 0.99).abs() < 1e-5,
            "{}",
            arrow.velocity
        );
        assert!(arrow.position.z < 0.0, "it moved back: {}", arrow.position);
    }

    #[test]
    fn an_arrow_that_hits_a_boat_is_destroyed() {
        let (_, fate) = fly_into(entity(2, EntityKind::Boat, DVec3::new(0.0, 64.0, 1.5)));
        assert_eq!(fate, Fate::Vanishes);
    }

    #[test]
    fn an_arrow_with_no_target_flies_on() {
        let (arrow, fate) = fly_into(entity(2, EntityKind::Pig, DVec3::new(0.0, 64.0, 50.0)));
        assert_eq!(fate, Fate::Stays);
        assert!(arrow.velocity.z > 1.9 && arrow.air_ticks == 10);
    }

    #[test]
    fn no_targets_are_gathered_while_no_arrow_flies() {
        let pig = entity(2, EntityKind::Pig, DVec3::ZERO);
        assert!(arrow_targets([&pig].into_iter()).is_empty());
    }

    #[test]
    fn a_segment_that_starts_inside_leaves_by_a_face() {
        let bounds = Aabb::new(DVec3::splat(-1.0), DVec3::splat(1.0));
        let distance = intercept(bounds, DVec3::ZERO, DVec3::new(0.0, 0.0, 3.0));
        assert_eq!(distance, Some(1.0));
    }

    #[test]
    fn a_segment_parallel_to_a_face_does_not_meet_it() {
        assert_eq!(
            at_plane(DVec3::ZERO, DVec3::new(0.0, 0.0, 3.0), 0, 1.0),
            None
        );
    }
}
