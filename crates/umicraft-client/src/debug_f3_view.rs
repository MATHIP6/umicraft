// SPDX-License-Identifier: MIT OR Apache-2.0

//! Assembles the vanilla F3 screen (P9-g) from what the window knows.
//!
//! The text rules live in `umicraft_game::debug_f3`; this module only maps
//! client state to its input. Values the render thread cannot see yet
//! (entities, particles, the block under the crosshair, light, biome, the GPU
//! adapter) stay at their defaults until P9-h moves the zone views to the game
//! thread; see `docs/parity.md`.

use umicraft_game::debug_f3::{DebugF3Input, DebugF3Lines};
use umicraft_render::{Camera, DebugF3View};

use crate::options::{VideoOptions, UNLIMITED_FPS};

/// The eye height of a standing player (`EntityPlayer.getEyeHeight`), used to
/// get the feet position `XYZ:` prints from the camera.
const EYE_HEIGHT: f64 = 1.62;

/// The numbers the window hands over each frame.
#[derive(Debug, Clone, Copy)]
pub struct F3Frame<'a> {
    /// The smoothed frames per second.
    pub fps: f32,
    /// Chunk meshes resident on the GPU.
    pub chunk_meshes: usize,
    /// The video options (render distance, graphics, frame cap).
    pub video: &'a VideoOptions,
    /// The window size in pixels.
    pub display: (u32, u32),
    /// The renderer camera.
    pub camera: &'a Camera,
}

/// Maps the renderer camera to the Minecraft yaw and pitch in degrees.
///
/// The camera is turned half a turn and has its pitch flipped relative to the
/// game (`game::camera_angles`); this is the inverse.
fn game_angles(camera: &Camera) -> (f32, f32) {
    (
        (camera.yaw - std::f32::consts::PI).to_degrees(),
        (0.0 - camera.pitch).to_degrees(),
    )
}

/// Builds the two columns for the render thread.
#[must_use]
pub fn build(frame: &F3Frame<'_>) -> DebugF3View {
    let (yaw, pitch) = game_angles(frame.camera);
    let position = frame.camera.position;
    let input = DebugF3Input {
        fps: frame.fps.round() as u32,
        fps_limit: (frame.video.max_fps < UNLIMITED_FPS).then_some(frame.video.max_fps),
        fancy_graphics: frame.video.fancy_graphics,
        clouds: match (frame.video.clouds, frame.video.fancy_graphics) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        },
        chunks_rendered: frame.chunk_meshes,
        chunks_total: frame.chunk_meshes,
        render_distance: u32::from(frame.video.render_distance),
        feet: [
            f64::from(position.x),
            f64::from(position.y) - EYE_HEIGHT,
            f64::from(position.z),
        ],
        yaw,
        pitch,
        display: frame.display,
        gpu_name: "wgpu".to_owned(),
        gpu_vendor: "Umicraft".to_owned(),
        gpu_backend: "wgpu".to_owned(),
        ..DebugF3Input::default()
    };
    let lines = DebugF3Lines::build(&input);
    DebugF3View {
        left: lines.left,
        right: lines.right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_maps_to_the_game_angles_and_feet() {
        let mut camera = Camera::default();
        camera.position = glam::Vec3::new(1.5, 65.62, -2.0);
        camera.yaw = std::f32::consts::PI;
        camera.pitch = 0.0;
        let video = VideoOptions::default();
        let view = build(&F3Frame {
            fps: 59.6,
            chunk_meshes: 3,
            video: &video,
            display: (854, 480),
            camera: &camera,
        });
        assert!(view.left[1].starts_with("60 fps"), "{}", view.left[1]);
        assert!(view
            .left
            .contains(&"XYZ: 1.500 / 64.00000 / -2.000".to_owned()));
        assert!(view
            .left
            .contains(&"Facing: south (Towards positive Z) (0.0 / 0.0)".to_owned()));
        assert!(view
            .right
            .contains(&"Display: 854x480 (Umicraft)".to_owned()));
    }
}
