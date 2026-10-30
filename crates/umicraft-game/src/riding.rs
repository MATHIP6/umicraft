// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where a rider sits on its vehicle.
//!
//! A riding player does not run its own physics: every tick the vehicle puts
//! it at the seat (MCP `Entity.updateRidden` calls
//! `ridingEntity.updateRiderPosition`), and the server moves the vehicle
//! from the `Steer Vehicle` keys. This module only holds the seat geometry.

use glam::DVec3;

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
/// three quarters of its height (MCP `Entity.getMountedYOffset`). The horse's
/// rearing shift is not modelled.
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
    let (shift, seat) = match vehicle.kind {
        EntityKind::Boat => {
            let yaw = f64::from(vehicle.yaw).to_radians();
            (
                DVec3::new(yaw.cos(), 0.0, yaw.sin()) * BOAT_SEAT_SHIFT,
                BOAT_SEAT_HEIGHT,
            )
        }
        EntityKind::Minecart => (DVec3::ZERO, MINECART_SEAT_HEIGHT),
        EntityKind::Spider | EntityKind::CaveSpider => {
            (DVec3::ZERO, f64::from(vehicle.kind.height()) * 0.5)
        }
        kind => (DVec3::ZERO, f64::from(kind.height()) * 0.75),
    };
    vehicle.position + shift + DVec3::Y * (seat + PLAYER_Y_OFFSET)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn a_minecart_seats_the_rider_at_its_floor() {
        let feet = rider_feet(&record(EntityKind::Minecart, 45.0));
        assert_eq!((feet.x, feet.z), (10.0, 20.0));
        assert!((feet.y - (64.0 - 0.35)).abs() < 1e-9);
    }

    #[test]
    fn animals_seat_the_rider_by_their_height() {
        assert!((rider_feet(&record(EntityKind::Pig, 0.0)).y - 64.325).abs() < 1e-6);
        assert!((rider_feet(&record(EntityKind::Horse, 0.0)).y - 64.85).abs() < 1e-6);
        assert!((rider_feet(&record(EntityKind::Spider, 0.0)).y - 64.1).abs() < 1e-6);
    }
}
