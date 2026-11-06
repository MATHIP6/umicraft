// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where a rider sits on its vehicle.
//!
//! A riding player does not run its own physics: every tick the vehicle puts
//! it at the seat (MCP `Entity.updateRidden` calls
//! `ridingEntity.updateRiderPosition`), and the server moves the vehicle
//! from the `Steer Vehicle` keys. This module only holds the seat geometry.

use glam::DVec3;

use crate::entity::meta::Variant;
use crate::entity::{EntityKind, EntityRecord};

/// MCP `EntityPlayer.getYOffset`: the rider's feet sit 0.35 below the seat.
const PLAYER_Y_OFFSET: f64 = -0.35;
/// MCP `EntityBoat.getMountedYOffset`.
const BOAT_SEAT_HEIGHT: f64 = -0.3;
/// How far the seat is shifted along the boat's yaw
/// (MCP `EntityBoat.updateRiderPosition`).
const BOAT_SEAT_SHIFT: f64 = 0.4;
/// MCP `EntityMinecart.getMountedYOffset`.
const MINECART_SEAT_HEIGHT: f64 = 0.0;

/// The position of the rider's feet when it rides `vehicle`.
///
/// A boat shifts the seat 0.4 along its yaw, a spider rides at half its
/// height, a minecart at its floor and every other mount (pig, horse) at
/// three quarters of its height (MCP `Entity.getMountedYOffset`). A rearing
/// horse pushes the seat back and up (`EntityHorse.updateRiderPosition`).
///
/// # Example
///
/// ```
/// use glam::DVec3;
/// use umicraft_game::entity::{EntityId, EntityKind, EntityRecord};
/// use umicraft_game::riding::rider_feet;
///
/// let pig = EntityRecord::new(EntityId(1), EntityKind::Pig, DVec3::new(2.0, 64.0, 2.0), 0.0, 0.0);
/// // 0.9 * 0.75 - 0.35 above the pig's feet.
/// assert!((rider_feet(&pig).y - 64.325).abs() < 1e-6);
/// ```
#[must_use]
pub fn rider_feet(vehicle: &EntityRecord) -> DVec3 {
    seat(vehicle) + DVec3::Y * PLAYER_Y_OFFSET
}

/// The position of `rider` when it rides `vehicle`: the seat plus the
/// rider's own `getYOffset`. A dropped item on an invisible mount, as the
/// item lines of a hologram plugin, sits right on the seat.
#[must_use]
pub fn rider_position(vehicle: &EntityRecord, rider: &EntityRecord) -> DVec3 {
    seat(vehicle) + DVec3::Y * rider_offset(rider)
}

/// MCP `Entity.getYOffset` of a rider: players, zombies and skeletons sit
/// 0.35 lower (a child zombie does not), a silverfish 0.2 higher.
fn rider_offset(rider: &EntityRecord) -> f64 {
    match rider.kind {
        EntityKind::Player => PLAYER_Y_OFFSET,
        EntityKind::Zombie
        | EntityKind::ZombiePigman
        | EntityKind::ZombieVillager
        | EntityKind::Skeleton => {
            if rider.meta.baby {
                0.0
            } else {
                PLAYER_Y_OFFSET
            }
        }
        EntityKind::Silverfish => 0.2,
        _ => 0.0,
    }
}

/// Where a rider is put when it leaves `vehicle` (`Entity.mountEntity(null)`):
/// right over it, at the top of its hitbox.
#[must_use]
pub fn dismount_position(vehicle: &EntityRecord) -> DVec3 {
    vehicle.position + DVec3::Y * mount_height(vehicle)
}

/// The hitbox height `getMountedYOffset` takes three quarters of: a marker
/// armour stand has no size (`EntityArmorStand.func_181550_a`).
fn mount_height(vehicle: &EntityRecord) -> f64 {
    match &vehicle.meta.variant {
        Variant::ArmorStand(stand) if stand.marker() => 0.0,
        _ => f64::from(vehicle.kind.height()),
    }
}

/// The seat of `vehicle` (MCP `Entity.updateRiderPosition`: the vehicle's
/// position plus `getMountedYOffset`), before the rider's own offset.
fn seat(vehicle: &EntityRecord) -> DVec3 {
    let (shift, seat) = match vehicle.kind {
        EntityKind::Boat => {
            let yaw = f64::from(vehicle.yaw).to_radians();
            (
                DVec3::new(yaw.cos(), 0.0, yaw.sin()) * BOAT_SEAT_SHIFT,
                BOAT_SEAT_HEIGHT,
            )
        }
        EntityKind::Minecart => (DVec3::ZERO, MINECART_SEAT_HEIGHT),
        // `EntityChicken.updateRiderPosition`: half its height, 0.1 behind
        // the way its body points.
        EntityKind::Chicken => {
            let yaw = f64::from(vehicle.body_yaw.offset).to_radians();
            (
                DVec3::new(0.1 * yaw.sin(), 0.0, -0.1 * yaw.cos()),
                f64::from(vehicle.kind.height()) * 0.5,
            )
        }
        EntityKind::Spider | EntityKind::CaveSpider => {
            (DVec3::ZERO, f64::from(vehicle.kind.height()) * 0.5)
        }
        EntityKind::Horse => {
            let rearing = f64::from(vehicle.horse.previous_rearing());
            let yaw = f64::from(vehicle.body_yaw.offset).to_radians();
            (
                DVec3::new(
                    0.7 * rearing * yaw.sin(),
                    0.15 * rearing,
                    -0.7 * rearing * yaw.cos(),
                ),
                f64::from(vehicle.kind.height()) * 0.75,
            )
        }
        _ => (DVec3::ZERO, mount_height(vehicle) * 0.75),
    };
    vehicle.position + shift + DVec3::Y * seat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::meta::ArmorStandMeta;
    use crate::entity::EntityId;

    fn record(kind: EntityKind, yaw: f32) -> EntityRecord {
        EntityRecord::new(EntityId(1), kind, DVec3::new(10.0, 64.0, 20.0), yaw, 0.0)
    }

    #[test]
    fn a_boat_seats_the_rider_low_and_shifted_along_its_yaw() {
        let feet = rider_feet(&record(EntityKind::Boat, 0.0));
        assert!((feet.x - 10.4).abs() < 1e-9);
        assert!((feet.y - (64.0 - 0.3 - 0.35)).abs() < 1e-9);
        assert!((feet.z - 20.0).abs() < 1e-9);
        let turned = rider_feet(&record(EntityKind::Boat, 90.0));
        assert!((turned.x - 10.0).abs() < 1e-9);
        assert!((turned.z - 20.4).abs() < 1e-9);
    }

    #[test]
    fn a_rearing_horse_throws_its_rider_back_and_up() {
        let mut horse = record(EntityKind::Horse, 0.0);
        let calm = rider_feet(&horse);
        for _ in 0..8 {
            horse.horse.tick(false, true, false);
        }
        let rearing = rider_feet(&horse);
        assert!((rearing.y - calm.y - 0.15).abs() < 1e-6);
        // Facing south (yaw 0), the back is north: z decreases by 0.7.
        assert!((rearing.z - calm.z + 0.7).abs() < 1e-6, "{rearing} {calm}");
        assert!((rearing.x - calm.x).abs() < 1e-6);
    }

    #[test]
    fn a_minecart_seats_the_rider_at_its_floor() {
        let feet = rider_feet(&record(EntityKind::Minecart, 45.0));
        assert_eq!((feet.x, feet.z), (10.0, 20.0));
        assert!((feet.y - (64.0 - 0.35)).abs() < 1e-9);
    }

    #[test]
    fn a_dropped_item_sits_on_the_seat_of_a_wither_skull() {
        // A hologram item line: 0.3125 * 0.75 above the skull, no offset.
        let skull = record(EntityKind::WitherSkull, 0.0);
        let item = record(EntityKind::Item, 0.0);
        let seated = rider_position(&skull, &item);
        assert!((seated.y - (64.0 + 0.3125 * 0.75)).abs() < 1e-6, "{seated}");
        // A player on the same mount is 0.35 lower, as `rider_feet` says.
        let player = record(EntityKind::Player, 0.0);
        assert_eq!(rider_position(&skull, &player), rider_feet(&skull));
    }

    #[test]
    fn an_item_sits_at_the_foot_of_a_marker_stand_and_high_on_a_normal_one() {
        let item = record(EntityKind::Item, 0.0);
        let stand = |flags: u8| {
            let mut stand = record(EntityKind::ArmorStand, 0.0);
            stand.meta.variant = Variant::ArmorStand(ArmorStandMeta {
                flags,
                ..ArmorStandMeta::default()
            });
            stand
        };
        assert!((rider_position(&stand(16), &item).y - 64.0).abs() < 1e-9);
        assert!((rider_position(&stand(0), &item).y - (64.0 + 1.975 * 0.75)).abs() < 1e-6);
    }

    #[test]
    fn a_chicken_seats_the_rider_at_half_its_height_a_little_behind() {
        // `EntityChicken.updateRiderPosition`: height * 0.5 and 0.1 back along
        // the body; a baby zombie (offset 0 for a child) rides it.
        let chicken = record(EntityKind::Chicken, 0.0);
        let mut jockey = record(EntityKind::Zombie, 0.0);
        jockey.meta.baby = true;
        let seated = rider_position(&chicken, &jockey);
        assert!((seated.y - (64.0 + 0.7 * 0.5)).abs() < 1e-6, "{seated}");
        assert!((seated.x - 10.0).abs() < 1e-9);
        assert!((seated.z - (20.0 - 0.1)).abs() < 1e-9);
    }

    #[test]
    fn animals_seat_the_rider_by_their_height() {
        assert!((rider_feet(&record(EntityKind::Pig, 0.0)).y - 64.325).abs() < 1e-6);
        assert!((rider_feet(&record(EntityKind::Horse, 0.0)).y - 64.85).abs() < 1e-6);
        assert!((rider_feet(&record(EntityKind::Spider, 0.0)).y - 64.1).abs() < 1e-6);
    }
}
