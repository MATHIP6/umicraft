// SPDX-License-Identifier: MIT OR Apache-2.0

//! The units the entity packets carry their positions, angles and
//! velocities in (MCP `NetHandlerPlayClient`, `EntityTracker`).

use glam::{DVec3, Vec3};

/// Wire units per block of a position: positions travel as fixed-point
/// 1/32 block (MCP `handleSpawnMob`: `posX / 32.0D`).
const POSITION_UNITS: f64 = 32.0;

/// Wire units of `Entity Velocity` per block per tick
/// (MCP `handleEntityVelocity`: `/ 8000`).
pub(in crate::game) const VELOCITY_UNITS: f64 = 8000.0;

/// A position of the wire, absolute or relative, in blocks.
pub(super) fn position<T: Into<f64>>(x: T, y: T, z: T) -> DVec3 {
    DVec3::new(
        x.into() / POSITION_UNITS,
        y.into() / POSITION_UNITS,
        z.into() / POSITION_UNITS,
    )
}

/// An angle of the wire, a byte of 256 steps per turn, in degrees
/// (MCP `handleSpawnMob`: `yaw * 360 / 256.0F`).
pub(super) fn angle(steps: i8) -> f32 {
    f32::from(steps) * 360.0 / 256.0
}

/// The velocity of an entity of the wire, in blocks per tick.
pub(super) fn velocity(x: i16, y: i16, z: i16) -> Vec3 {
    let units = VELOCITY_UNITS as f32;
    Vec3::new(
        f32::from(x) / units,
        f32::from(y) / units,
        f32::from(z) / units,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_in_thirty_seconds_of_a_block() {
        assert_eq!(position(32, -48, 8), DVec3::new(1.0, -1.5, 0.25));
        assert_eq!(position(16_i8, 0, -32), DVec3::new(0.5, 0.0, -1.0));
    }

    #[test]
    fn an_angle_byte_is_a_256th_of_a_turn() {
        assert_eq!(angle(64), 90.0);
        assert_eq!(angle(-128), -180.0);
    }

    #[test]
    fn a_velocity_is_in_8000ths_of_a_block_per_tick() {
        assert_eq!(velocity(8000, -4000, 0), Vec3::new(1.0, -0.5, 0.0));
    }
}
