// SPDX-License-Identifier: MIT OR Apache-2.0

//! The seven boxes of the ender dragon (MCP `EntityDragon.dragonPartArray`).
//!
//! The dragon itself answers false to `canBeCollidedWith`: the crosshair and
//! arrows only meet its parts, the head, the body, three tail segments and
//! two wings. `EntityDragon.onLivingUpdate` places them every tick on the
//! client too, from the yaw, the position and the flight history. They are
//! not in the entity list, but `Chunk.getEntitiesWithinAABBForEntity` adds
//! them next to their dragon. `NetHandlerPlayClient.handleSpawnMob` gives
//! them the server's ids, the dragon's id plus 1 to 7 in array order, so an
//! attack names the part that was hit.

use glam::DVec3;

use super::angle::wrap_degrees;
use super::kind::{EntityId, EntityKind};
use super::record::EntityRecord;
use crate::collision::Aabb;

/// How many parts a dragon has.
pub const PARTS: usize = 7;

/// The width and height of each part, in array order: head, body, three
/// tail segments, two wings. They are the values `onLivingUpdate` writes
/// before it places the parts, not those of the constructor.
const SIZES: [(f64, f64); PARTS] = [
    (3.0, 3.0),
    (5.0, 3.0),
    (2.0, 2.0),
    (2.0, 2.0),
    (2.0, 2.0),
    (4.0, 2.0),
    (4.0, 3.0),
];

/// One part of a dragon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragonPart {
    /// The id the server knows the part by.
    pub id: EntityId,
    /// The middle of the bottom face (`posX`, `posY`, `posZ` of the part).
    pub feet: DVec3,
    /// The box of the part, without any border.
    pub bounds: Aabb,
}

/// The parts of `dragon` where its last tick left them; `None` for any
/// other entity, for a dragon that has not ticked yet and for a dead one,
/// whose parts `onLivingUpdate` no longer moves.
#[must_use]
pub fn dragon_parts(dragon: &EntityRecord) -> Option<[DragonPart; PARTS]> {
    if dragon.kind != EntityKind::Dragon || dragon.health.is_some_and(|health| health <= 0.0) {
        return None;
    }
    let flight = dragon.flight.as_ref()?;
    let offsets = |back: usize| flight.offsets(back, 1.0, true);
    let feet = part_feet(dragon.position, dragon.yaw, offsets);
    Some(std::array::from_fn(|index| {
        let (width, height) = SIZES[index];
        let half = width / 2.0;
        let at = feet[index];
        DragonPart {
            id: EntityId(dragon.id.0.wrapping_add(index as i32 + 1)),
            feet: at,
            bounds: Aabb::new(
                DVec3::new(at.x - half, at.y, at.z - half),
                DVec3::new(at.x + half, at.y + height, at.z + half),
            ),
        }
    }))
}

/// The dragon `id` is a part of, if `id` is the id of a part of a dragon in
/// `entities`.
#[must_use]
pub fn owner_of_part(
    id: EntityId,
    entities: &std::collections::BTreeMap<EntityId, EntityRecord>,
) -> Option<&EntityRecord> {
    (1..=PARTS as i32).find_map(|index| {
        entities
            .get(&EntityId(id.0.wrapping_sub(index)))
            .filter(|owner| owner.kind == EntityKind::Dragon)
    })
}

/// Where `onLivingUpdate` puts each part, in array order, for a dragon at
/// `position` facing `yaw` with the flight history `offsets` (the yaw and
/// height `back` ticks ago, `getMovementOffsets(back, 1.0F)`).
///
/// `randomYawVelocity` turns the head a little on the server; the client
/// never sets it, so it is 0 here.
fn part_feet(position: DVec3, yaw: f32, offsets: impl Fn(usize) -> [f64; 2]) -> [DVec3; PARTS] {
    let [x, y, z] = position.to_array();
    let now = offsets(0);
    let five = offsets(5);
    let tilt = ((five[1] - offsets(10)[1]) as f32 * 10.0).to_radians();
    let (f2, f15) = (tilt.cos(), -tilt.sin());
    let facing = yaw.to_radians();
    let (f16, f4) = (facing.sin(), facing.cos());
    let head = DVec3::new(
        x + f64::from(f16 * 5.5 * f2),
        y + (now[1] - five[1]) + f64::from(f15 * 5.5),
        z - f64::from(f4 * 5.5 * f2),
    );
    let body = DVec3::new(x + f64::from(f16 * 0.5), y, z - f64::from(f4 * 0.5));
    let tail = |segment: usize| {
        let past = offsets(12 + segment * 2);
        let turn = facing + wrap_degrees((past[0] - five[0]) as f32).to_radians();
        let (f21, f22) = (turn.sin(), turn.cos());
        let (f23, f24) = (1.5_f32, (segment + 1) as f32 * 2.0);
        DVec3::new(
            x - f64::from((f16 * f23 + f21 * f24) * f2),
            y + (past[1] - five[1]) - f64::from((f24 + f23) * f15) + 1.5,
            z + f64::from((f4 * f23 + f22 * f24) * f2),
        )
    };
    let wing = |side: f32| {
        DVec3::new(
            x + f64::from(side * f4 * 4.5),
            y + 2.0,
            z + f64::from(side * f16 * 4.5),
        )
    };
    [head, body, tail(0), tail(1), tail(2), wing(1.0), wing(-1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn dragon(yaw: f32) -> EntityRecord {
        let mut dragon = EntityRecord::new(
            EntityId(40),
            EntityKind::Dragon,
            DVec3::new(0.0, 70.0, 0.0),
            yaw,
            0.0,
        );
        let mut flight = super::super::flight::DragonFlight::default();
        flight.tick(yaw, 70.0, glam::Vec3::ZERO, true);
        dragon.flight = Some(flight);
        dragon
    }

    fn close(a: DVec3, b: DVec3) -> bool {
        a.distance(b) < 1e-5
    }

    #[test]
    fn a_level_dragon_facing_south_has_its_head_north_and_its_tail_south() {
        // Yaw 0: `f16 = 0`, `f4 = 1`; the head is 5.5 towards -z, the tail
        // segments 3.5, 5.5 and 7.5 towards +z, 1.5 above the feet.
        let parts = dragon_parts(&dragon(0.0)).expect("parts");
        assert!(close(parts[0].feet, DVec3::new(0.0, 70.0, -5.5)));
        assert!(close(parts[1].feet, DVec3::new(0.0, 70.0, -0.5)));
        for (segment, z) in [3.5, 5.5, 7.5].into_iter().enumerate() {
            assert!(close(parts[2 + segment].feet, DVec3::new(0.0, 71.5, z)));
        }
        assert!(close(parts[5].feet, DVec3::new(4.5, 72.0, 0.0)));
        assert!(close(parts[6].feet, DVec3::new(-4.5, 72.0, 0.0)));
    }

    #[test]
    fn the_parts_have_the_sizes_of_the_tick_and_the_ids_after_the_dragon() {
        let parts = dragon_parts(&dragon(0.0)).expect("parts");
        let ids: Vec<i32> = parts.iter().map(|part| part.id.0).collect();
        assert_eq!(ids, [41, 42, 43, 44, 45, 46, 47]);
        let body = parts[1].bounds;
        assert_eq!(body.max - body.min, DVec3::new(5.0, 3.0, 5.0));
        let head = parts[0].bounds;
        assert_eq!(head.max - head.min, DVec3::new(3.0, 3.0, 3.0));
    }

    #[test]
    fn the_parts_turn_with_the_dragon() {
        // Yaw 90: `f16 = 1`, `f4 = 0`; the head is 5.5 towards +x.
        let parts = dragon_parts(&dragon(90.0)).expect("parts");
        assert!(close(parts[0].feet, DVec3::new(5.5, 70.0, 0.0)));
        assert!(close(parts[5].feet, DVec3::new(0.0, 72.0, 4.5)));
    }

    #[test]
    fn only_a_living_dragon_that_ticked_has_parts() {
        let mut fresh = dragon(0.0);
        fresh.flight = None;
        assert!(dragon_parts(&fresh).is_none());
        let mut dead = dragon(0.0);
        dead.health = Some(0.0);
        assert!(dragon_parts(&dead).is_none());
        let mut zombie = dragon(0.0);
        zombie.kind = EntityKind::Zombie;
        assert!(dragon_parts(&zombie).is_none());
    }

    #[test]
    fn a_part_id_leads_back_to_its_dragon() {
        let entities = BTreeMap::from([(EntityId(40), dragon(0.0))]);
        for id in 41..=47 {
            assert_eq!(
                owner_of_part(EntityId(id), &entities).map(|owner| owner.id),
                Some(EntityId(40))
            );
        }
        assert!(owner_of_part(EntityId(40), &entities).is_none());
        assert!(owner_of_part(EntityId(48), &entities).is_none());
    }
}
