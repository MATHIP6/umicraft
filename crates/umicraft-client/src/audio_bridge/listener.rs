// SPDX-License-Identifier: MIT OR Apache-2.0

//! The ears of the player: the render camera as an audio [`Listener`].
//!
//! Vanilla sets the listener from the view entity every frame
//! (`SoundHandler.setListener` -> `SoundManager.setListener`), so the pan
//! follows the mouse look even between two game ticks.

use std::f32::consts::PI;

use glam::Vec3;
use umicraft_audio::Listener;

/// The listener of a camera at `position` with the renderer's `camera_yaw`.
///
/// The renderer yaw is the game yaw turned by half a turn
/// (`game::camera_angles`); the audio listener uses the game convention, so
/// the half turn is undone and the angle given in degrees.
#[must_use]
pub fn listener_of(position: Vec3, camera_yaw: f32) -> Listener {
    Listener {
        position: [
            f64::from(position.x),
            f64::from(position.y),
            f64::from(position.z),
        ],
        yaw: (camera_yaw - PI).to_degrees(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_position_is_kept_in_world_coordinates() {
        let listener = listener_of(Vec3::new(1.0, 65.62, -3.0), PI);
        assert_eq!(listener.position, [1.0, f64::from(65.62_f32), -3.0]);
    }

    #[test]
    fn the_half_turn_of_the_renderer_is_undone() {
        // Game yaw 0 (facing +z) is camera yaw PI.
        assert!(listener_of(Vec3::ZERO, PI).yaw.abs() < 1e-4);
        // Game yaw 90 degrees (facing -x).
        let yaw = listener_of(Vec3::ZERO, PI + 90.0_f32.to_radians()).yaw;
        assert!((yaw - 90.0).abs() < 1e-3, "{yaw}");
    }

    #[test]
    fn a_source_to_the_west_is_on_the_right_when_facing_south() {
        let listener = listener_of(Vec3::ZERO, PI);
        assert!(listener.pan_to([-5.0, 0.0, 0.0]) > 0.99);
    }
}
