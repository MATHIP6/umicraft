// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client-side motion of objects between the packets of the server.
//!
//! The server sends the position of an arrow, a dropped item or a falling
//! block every 20 ticks at most; in between, the client runs the same physics
//! as the reference (`EntityItem`, `EntityFallingBlock`, `EntityTNTPrimed`,
//! `EntityArrow`, `EntityThrowable`, `EntityFireball`), with the collisions of
//! its own world, so they fly, fall and land by themselves.

use glam::{DVec3, Vec3};

use super::kind::EntityKind;
use super::object_hits::{self, Hit, Target};
use super::object_liquid::{self, Scan};
use super::record::EntityRecord;
use crate::collision::{move_entity, Aabb, MoveOptions};
use crate::raycast::raycast;
use crate::shapes::BlockSource;

/// Block ids with their own slipperiness (`Block.slipperiness`, 0.6 by default).
const ICE: u16 = 79;
const PACKED_ICE: u16 = 174;
const SLIME_BLOCK: u16 = 165;

/// How far back from the block an arrow stops (`EntityArrow`, `0.05`).
const STUCK_BACK_OFF: f64 = 0.05;

/// The size of the box of an object: its width and height (`setSize`).
fn size(kind: EntityKind) -> (f64, f64) {
    match kind {
        EntityKind::FallingBlock | EntityKind::PrimedTnt => (0.98, 0.98),
        EntityKind::Arrow => (0.5, 0.5),
        EntityKind::LargeFireball => (1.0, 1.0),
        EntityKind::SmallFireball | EntityKind::WitherSkull => (0.3125, 0.3125),
        _ => (0.25, 0.25),
    }
}

/// The box of an object around its feet.
pub(super) fn bounds(entity: &EntityRecord) -> Aabb {
    let (width, height) = size(entity.kind);
    let half = width / 2.0;
    let feet = entity.position;
    Aabb::new(
        DVec3::new(feet.x - half, feet.y, feet.z - half),
        DVec3::new(feet.x + half, feet.y + height, feet.z + half),
    )
}

/// What became of an object this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// It stays in the world.
    Stays,
    /// It is destroyed (an arrow that hit a boat or a cart).
    Vanishes,
}

/// The gravity of a thrown object, per tick (`getGravityVelocity`).
fn thrown_gravity(kind: EntityKind) -> Option<f32> {
    match kind {
        EntityKind::Snowball | EntityKind::Egg | EntityKind::EnderPearl => Some(0.03),
        EntityKind::Potion => Some(0.05),
        EntityKind::ExpBottle => Some(0.07),
        _ => None,
    }
}

/// Advances `entity` by one tick when it is an object the client simulates.
///
/// `targets` are the entities an arrow can hit, from [`arrow_targets`].
pub(super) fn step<S: BlockSource + ?Sized>(
    entity: &mut EntityRecord,
    world: &S,
    targets: &[Target],
) -> Fate {
    match entity.kind {
        EntityKind::Item => item(entity, world),
        EntityKind::FallingBlock | EntityKind::PrimedTnt => falling(entity, world),
        EntityKind::Arrow => return arrow(entity, world, targets),
        EntityKind::LargeFireball | EntityKind::SmallFireball | EntityKind::WitherSkull => {
            fireball(entity, world);
        }
        kind => {
            if let Some(gravity) = thrown_gravity(kind) {
                thrown(entity, world, gravity);
            }
        }
    }
    Fate::Stays
}

/// The entities an arrow can hit this tick, or nothing when no arrow flies
/// (which keeps the common tick free of allocation).
pub(super) fn arrow_targets<'a>(
    entities: impl Iterator<Item = &'a EntityRecord> + Clone,
) -> Vec<Target> {
    let flying = entities
        .clone()
        .any(|entity| entity.kind == EntityKind::Arrow && entity.velocity != Vec3::ZERO);
    if flying {
        object_hits::targets(entities)
    } else {
        Vec::new()
    }
}

/// Moves the box of `entity` by its velocity with the collisions of the
/// world; a blocked axis stops its velocity (`Entity.moveEntity`).
fn move_with_collisions<S: BlockSource + ?Sized>(entity: &mut EntityRecord, world: &S) {
    let result = move_entity(
        bounds(entity),
        entity.velocity.as_dvec3(),
        world,
        MoveOptions::default(),
    );
    entity.position += result.delta;
    entity.target_position = entity.position;
    if result.blocked_x {
        entity.velocity.x = 0.0;
    }
    if result.blocked_z {
        entity.velocity.z = 0.0;
    }
    if result.blocked_vertically {
        entity.velocity.y = 0.0;
    }
    entity.on_ground = result.on_ground;
}

/// `Block.slipperiness` of the block under the box.
fn slipperiness<S: BlockSource + ?Sized>(entity: &EntityRecord, world: &S) -> f32 {
    let under = world.block(
        entity.position.x.floor() as i32,
        entity.position.y.floor() as i32 - 1,
        entity.position.z.floor() as i32,
    );
    match under.id() {
        ICE | PACKED_ICE => 0.98,
        SLIME_BLOCK => 0.8,
        _ => 0.6,
    }
}

/// `EntityItem.onUpdate`: gravity 0.04, then the box moves, lava throws it up,
/// and the drag is the friction of the block it rests on. The current of the
/// water pushes it twice, at the start (`Entity.onUpdate`) and at the end.
fn item<S: BlockSource + ?Sized>(entity: &mut EntityRecord, world: &S) {
    object_liquid::handle_water(entity, world, Scan::Whole);
    entity.velocity.y -= 0.04;
    let before = entity.position;
    move_with_collisions(entity, world);
    object_liquid::kick_out_of_lava(entity, before, world);
    let friction = if entity.on_ground {
        slipperiness(entity, world) * 0.98
    } else {
        0.98
    };
    entity.velocity.x *= friction;
    entity.velocity.y *= 0.98;
    entity.velocity.z *= friction;
    if entity.on_ground {
        entity.velocity.y *= -0.5;
    }
    object_liquid::handle_water(entity, world, Scan::Whole);
}

/// `EntityFallingBlock.onUpdate` and `EntityTNTPrimed.onUpdate`: gravity 0.04,
/// the box moves, drag 0.98, and a landing slows the sliding. The current of
/// the water pushes both at the start (`Entity.onUpdate`) and a primed TNT
/// again at the end.
fn falling<S: BlockSource + ?Sized>(entity: &mut EntityRecord, world: &S) {
    object_liquid::handle_water(entity, world, Scan::Shrunk);
    entity.velocity.y -= 0.04;
    move_with_collisions(entity, world);
    entity.velocity *= 0.98;
    if entity.on_ground {
        entity.velocity.x *= 0.7;
        entity.velocity.z *= 0.7;
        entity.velocity.y *= -0.5;
    }
    if entity.kind == EntityKind::PrimedTnt {
        object_liquid::handle_water(entity, world, Scan::Shrunk);
    }
}

/// The yaw and pitch of a motion, in degrees (`EntityArrow.onUpdate`).
fn heading(velocity: Vec3) -> (f32, f32) {
    let horizontal = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
    (
        velocity.x.atan2(velocity.z).to_degrees(),
        velocity.y.atan2(horizontal).to_degrees(),
    )
}

/// Moves a point along its velocity and stops it at the first block on the
/// way (the ray trace of `EntityArrow` and `EntityThrowable`); `true` when it
/// hit one.
fn fly<S: BlockSource + ?Sized>(entity: &mut EntityRecord, world: &S) -> bool {
    let velocity = entity.velocity.as_dvec3();
    let distance = velocity.length();
    if distance == 0.0 {
        return false;
    }
    if let Some(hit) = raycast(entity.position, velocity, distance, world) {
        // Stop at the block, a little back along the flight like the arrow.
        let back = velocity / distance * STUCK_BACK_OFF;
        entity.position = hit.position - back;
        entity.target_position = entity.position;
        entity.velocity = Vec3::ZERO;
        return true;
    }
    entity.position += velocity;
    entity.target_position = entity.position;
    false
}

/// The first entity on the way of `arrow` this tick, if any: the segment of
/// its flight, cut at the first block, against the boxes of `targets`.
fn arrow_hit<S: BlockSource + ?Sized>(
    arrow: &EntityRecord,
    world: &S,
    targets: &[Target],
) -> Option<Hit> {
    if targets.is_empty() {
        return None;
    }
    let from = arrow.position;
    let velocity = arrow.velocity.as_dvec3();
    let end = raycast(from, velocity, velocity.length(), world)
        .map_or(from + velocity, |block| block.position);
    object_hits::first_hit(arrow, from, end, targets)
}

/// `EntityArrow.onUpdate` in the air: an entity on the way stops it (see
/// [`object_hits`]), then it points along its motion, moves, slows by 1
/// percent (40 percent in water) and drops 0.05 per tick; a block stops it.
fn arrow<S: BlockSource + ?Sized>(
    entity: &mut EntityRecord,
    world: &S,
    targets: &[Target],
) -> Fate {
    if entity.velocity == Vec3::ZERO {
        return Fate::Stays;
    }
    let wet = object_liquid::handle_water(entity, world, Scan::Shrunk);
    entity.air_ticks = entity.air_ticks.saturating_add(1);
    match arrow_hit(entity, world, targets) {
        Some(Hit::Absorb) => return Fate::Vanishes,
        Some(Hit::Bounce) => {
            entity.velocity *= -0.1;
            entity.air_ticks = 0;
        }
        None => {}
    }
    let (yaw, pitch) = heading(entity.velocity);
    entity.yaw = yaw;
    entity.pitch = pitch;
    if fly(entity, world) {
        return Fate::Stays;
    }
    entity.velocity *= if wet { 0.6 } else { 0.99 };
    entity.velocity.y -= 0.05;
    Fate::Stays
}

/// `EntityThrowable.onUpdate`: it moves, slows by 1 percent (20 percent in
/// water) and drops by the gravity of its class. The server tests the
/// entities on its way (`!isRemote`), the client only the blocks.
fn thrown<S: BlockSource + ?Sized>(entity: &mut EntityRecord, world: &S, gravity: f32) {
    if entity.velocity == Vec3::ZERO {
        return;
    }
    let wet = object_liquid::handle_water(entity, world, Scan::Shrunk);
    if fly(entity, world) {
        return;
    }
    entity.velocity *= if wet { 0.8 } else { 0.99 };
    entity.velocity.y -= gravity;
}

/// `EntityFireball.onUpdate`: it moves, speeds up along its direction by 0.1
/// and slows by 5 percent (20 percent in water).
fn fireball<S: BlockSource + ?Sized>(entity: &mut EntityRecord, world: &S) {
    if entity.velocity == Vec3::ZERO {
        return;
    }
    let wet = object_liquid::handle_water(entity, world, Scan::Shrunk);
    let direction = entity.velocity.normalize();
    if fly(entity, world) {
        return;
    }
    entity.velocity = (entity.velocity + direction * 0.1) * if wet { 0.8 } else { 0.95 };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::kind::EntityId;
    use umicraft_world::BlockState;

    /// A flat world: stone below `FLOOR`, air above.
    struct Flat;

    const FLOOR: i32 = 64;

    impl BlockSource for Flat {
        fn block(&self, _x: i32, y: i32, _z: i32) -> BlockState {
            if y < FLOOR {
                BlockState::new(1, 0)
            } else {
                BlockState::AIR
            }
        }
    }

    fn object(kind: EntityKind, position: DVec3, velocity: Vec3) -> EntityRecord {
        let mut entity = EntityRecord::new(EntityId(1), kind, position, 0.0, 0.0);
        entity.velocity = velocity;
        entity
    }

    #[test]
    fn a_dropped_item_falls_and_comes_to_rest_on_the_ground() {
        let mut item = object(EntityKind::Item, DVec3::new(0.5, 68.0, 0.5), Vec3::ZERO);
        for _ in 0..80 {
            step(&mut item, &Flat, &[]);
        }
        assert!(
            (item.position.y - f64::from(FLOOR)).abs() < 1e-6,
            "{}",
            item.position.y
        );
        assert!(item.on_ground);
        assert!(item.velocity.y.abs() < 0.05);
    }

    #[test]
    fn a_dropped_item_slides_and_stops_on_the_ground() {
        let mut item = object(
            EntityKind::Item,
            DVec3::new(0.5, 64.0, 0.5),
            Vec3::new(0.3, 0.0, 0.0),
        );
        for _ in 0..60 {
            step(&mut item, &Flat, &[]);
        }
        assert!(item.position.x > 0.5);
        assert!(item.velocity.x.abs() < 0.01, "friction stops it");
    }

    #[test]
    fn a_falling_block_accelerates_downwards() {
        let mut block = object(
            EntityKind::FallingBlock,
            DVec3::new(0.5, 80.0, 0.5),
            Vec3::ZERO,
        );
        step(&mut block, &Flat, &[]);
        let after_one = block.velocity.y;
        step(&mut block, &Flat, &[]);
        assert!(after_one < 0.0 && block.velocity.y < after_one);
    }

    #[test]
    fn an_arrow_follows_a_parabola_and_points_along_it() {
        let mut arrow = object(
            EntityKind::Arrow,
            DVec3::new(0.5, 80.0, 0.5),
            Vec3::new(0.0, 0.5, 1.0),
        );
        step(&mut arrow, &Flat, &[]);
        let early = arrow.pitch;
        assert!(early > 0.0, "it climbs first: {early}");
        for _ in 0..40 {
            step(&mut arrow, &Flat, &[]);
        }
        assert!(arrow.pitch < 0.0, "it comes down: {}", arrow.pitch);
        // Facing +Z: yaw 0.
        assert!(arrow.yaw.abs() < 1e-4);
        assert!(arrow.position.z > 10.0);
    }

    #[test]
    fn an_arrow_sticks_in_the_ground() {
        let mut arrow = object(
            EntityKind::Arrow,
            DVec3::new(0.5, 66.0, 0.5),
            Vec3::new(0.0, -0.8, 0.2),
        );
        for _ in 0..10 {
            step(&mut arrow, &Flat, &[]);
        }
        assert_eq!(arrow.velocity, Vec3::ZERO);
        assert!(arrow.position.y >= f64::from(FLOOR) - 0.1);
        // It stays where it is.
        let at = arrow.position;
        step(&mut arrow, &Flat, &[]);
        assert_eq!(arrow.position, at);
    }

    #[test]
    fn a_snowball_drops_less_than_a_potion_and_a_bottle() {
        let drop = |kind| {
            let mut thrown = object(kind, DVec3::new(0.5, 90.0, 0.5), Vec3::new(0.0, 0.0, 1.0));
            for _ in 0..10 {
                step(&mut thrown, &Flat, &[]);
            }
            90.0 - thrown.position.y
        };
        assert!(drop(EntityKind::Snowball) < drop(EntityKind::Potion));
        assert!(drop(EntityKind::Potion) < drop(EntityKind::ExpBottle));
    }

    #[test]
    fn a_fireball_speeds_up_in_a_straight_line() {
        let mut ball = object(
            EntityKind::LargeFireball,
            DVec3::new(0.5, 80.0, 0.5),
            Vec3::new(0.0, 0.0, 0.1),
        );
        let first = ball.velocity.z;
        for _ in 0..20 {
            step(&mut ball, &Flat, &[]);
        }
        assert!(ball.velocity.z > first);
        assert_eq!(ball.position.y, 80.0, "no gravity");
        assert!(ball.velocity.x == 0.0);
    }

    #[test]
    fn other_objects_are_left_to_the_server() {
        let mut boat = object(EntityKind::Boat, DVec3::new(0.5, 70.0, 0.5), Vec3::Y);
        step(&mut boat, &Flat, &[]);
        assert_eq!(boat.position, DVec3::new(0.5, 70.0, 0.5));
    }
}
