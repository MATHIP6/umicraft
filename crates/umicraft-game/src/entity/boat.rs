// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of a boat (MCP 1.8.9 `EntityBoat`).
//!
//! The reference client moves a boat two ways. A boat the local player does
//! not ride ("empty" on the client, `isBoatEmpty`) glides to each server
//! position over eight ticks, then keeps drifting with the velocity the server
//! sent, slowing down each tick. That drift is [`BoatMotion::drift`].

use glam::{DVec3, Vec3};

/// Ticks a boat the local player does not ride takes to reach a reported
/// position: the 3 of the packet plus 5 (`setPositionAndRotation2`).
pub const EMPTY_BOAT_INTERPOLATION_TICKS: u8 = 3 + 5;

/// What a drifting boat keeps of its motion each tick
/// (`0.99F`, `0.95F` and `0.99F` widened to `double`).
const DRIFT_DAMPING: DVec3 = DVec3::new(
    0.990_000_009_536_743_2,
    0.949_999_988_079_071,
    0.990_000_009_536_743_2,
);

/// The motion of a boat on the client (`motionX`, `motionY`, `motionZ`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BoatMotion {
    /// Blocks per tick.
    pub motion: DVec3,
}

impl BoatMotion {
    /// Takes the velocity the server sent (`setVelocity`, and
    /// `setPositionAndRotation2` copies it again on every move).
    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.motion = velocity.as_dvec3();
    }

    /// One tick of a boat that reached its last reported position: it moves
    /// by its motion, which halves on the ground and then fades
    /// (`EntityBoat.onUpdate`, `isRemote && isBoatEmpty`, no interpolation
    /// step left). Returns the new position.
    #[must_use]
    pub fn drift(&mut self, position: DVec3, on_ground: bool) -> DVec3 {
        let moved = position + self.motion;
        if on_ground {
            self.motion *= 0.5;
        }
        self.motion *= DRIFT_DAMPING;
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drifting_boat_moves_by_its_motion_then_slows_down() {
        let mut boat = BoatMotion::default();
        boat.set_velocity(Vec3::new(0.2, 0.0, 0.0));
        let next = boat.drift(DVec3::ZERO, false);
        assert!((next.x - 0.2).abs() < 1e-6);
        assert!((boat.motion.x - 0.2 * 0.99).abs() < 1e-6);
    }

    #[test]
    fn a_boat_on_the_ground_loses_half_its_motion_more() {
        let mut boat = BoatMotion::default();
        boat.set_velocity(Vec3::new(0.2, 0.0, 0.0));
        let _ = boat.drift(DVec3::ZERO, true);
        assert!((boat.motion.x - 0.1 * 0.99).abs() < 1e-6);
    }
}
