// SPDX-License-Identifier: MIT OR Apache-2.0

//! Assembles the vanilla F3 screen (P9-g) from what the window knows.
//!
//! The text rules live in `umicraft_game::debug_f3`; this module only maps
//! client state to its input. The world facts (biome, light, the block under
//! the crosshair, counts, the feet position) come from the game thread in
//! [`DebugWorld`]; the GPU names from [`AdapterSummary`].

use umicraft_game::debug_f3::{DebugF3Input, DebugF3Lines};
use umicraft_render::{AdapterSummary, Camera, DebugF3View, Renderer};

use crate::debug_world::DebugWorld;
use crate::process_memory::MemoryUse;

use crate::options::{VideoOptions, UNLIMITED_FPS};
use crate::world_view::WorldView;

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
    /// The world facts of the newest game frame.
    pub world: &'a DebugWorld,
    /// Entities and particles drawn in that frame.
    pub drawn: (usize, usize),
    /// The GPU adapter.
    pub adapter: &'a AdapterSummary,
    /// The process memory.
    pub memory: MemoryUse,
}

/// What only the window knows when F3 is drawn.
#[derive(Debug, Clone, Copy)]
pub struct Live<'a> {
    /// The smoothed frames per second.
    pub fps: f32,
    /// The video options (render distance, graphics, frame cap).
    pub video: &'a VideoOptions,
    /// The window size in pixels.
    pub display: (u32, u32),
    /// The renderer camera.
    pub camera: &'a Camera,
}

/// Builds the F3 columns from the live renderer and world, falling back to
/// empty values before either exists.
#[must_use]
pub fn build_live(
    renderer: Option<&Renderer>,
    world: Option<&WorldView>,
    live: &Live<'_>,
) -> DebugF3View {
    let no_world = DebugWorld::default();
    let no_adapter = AdapterSummary::default();
    build(&F3Frame {
        fps: live.fps,
        chunk_meshes: renderer.map_or(0, Renderer::mesh_count),
        video: live.video,
        display: live.display,
        camera: live.camera,
        world: world.map_or(&no_world, WorldView::debug_world),
        drawn: world.map_or((0, 0), WorldView::drawn_counts),
        adapter: renderer.map_or(&no_adapter, Renderer::adapter),
        memory: crate::process_memory::read(),
    })
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
        entities_rendered: frame.drawn.0,
        entities_total: frame.world.loaded_entities,
        entities_hidden: 0,
        particles: frame.drawn.1,
        loaded_entities: frame.world.loaded_entities,
        chunk_cache: frame.world.chunk_cache,
        feet: frame.world.feet,
        location: frame.world.location.clone(),
        looking_at: frame.world.looking_at.clone(),
        memory_used: frame.memory.used,
        memory_max: frame.memory.max,
        yaw,
        pitch,
        display: frame.display,
        gpu_name: frame.adapter.name.clone(),
        gpu_vendor: frame.adapter.vendor.clone(),
        gpu_backend: frame.adapter.backend.clone(),
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

    fn frame<'a>(
        camera: &'a Camera,
        video: &'a VideoOptions,
        world: &'a DebugWorld,
        adapter: &'a AdapterSummary,
    ) -> F3Frame<'a> {
        F3Frame {
            fps: 59.6,
            chunk_meshes: 3,
            video,
            display: (854, 480),
            camera,
            world,
            drawn: (4, 17),
            adapter,
            memory: MemoryUse {
                used: 300 * 1024 * 1024,
                max: 1024 * 1024 * 1024,
            },
        }
    }

    #[test]
    fn the_camera_maps_to_the_game_angles() {
        let mut camera = Camera::default();
        camera.yaw = std::f32::consts::PI;
        camera.pitch = 0.0;
        let view = build(&frame(
            &camera,
            &VideoOptions::default(),
            &DebugWorld::default(),
            &AdapterSummary::default(),
        ));
        assert!(view.left[1].starts_with("60 fps"), "{}", view.left[1]);
        assert!(view
            .left
            .contains(&"Facing: south (Towards positive Z) (0.0 / 0.0)".to_owned()));
        assert!(view.right[0].starts_with("Mem:  29% 300/1024MB"));
    }

    #[test]
    fn the_feet_come_from_the_game_not_from_the_camera() {
        // A sneaking player's eye is not 1.62 above the feet.
        let mut camera = Camera::default();
        camera.position = glam::Vec3::new(1.5, 65.27, -2.0);
        let world = DebugWorld {
            feet: [1.5, 64.0, -2.0],
            ..DebugWorld::default()
        };
        let view = build(&frame(
            &camera,
            &VideoOptions::default(),
            &world,
            &AdapterSummary::default(),
        ));
        assert!(view
            .left
            .contains(&"XYZ: 1.500 / 64.00000 / -2.000".to_owned()));
    }

    #[test]
    fn counts_biome_block_and_adapter_reach_the_screen() {
        let world = DebugWorld {
            loaded_entities: 9,
            chunk_cache: (441, 441),
            location: Some(umicraft_game::debug_f3::DebugLocation {
                biome: "Plains".to_owned(),
                light: 15,
                sky_light: 15,
                block_light: 0,
                local_difficulty: 1.5,
                day: 3,
            }),
            looking_at: Some(umicraft_game::debug_f3::LookedAtBlock {
                pos: [1, 2, 3],
                name: "minecraft:stone".to_owned(),
                properties: Vec::new(),
            }),
            ..DebugWorld::default()
        };
        let adapter = AdapterSummary {
            name: "Test GPU".to_owned(),
            vendor: "radv".to_owned(),
            backend: "Vulkan".to_owned(),
        };
        let camera = Camera::default();
        let view = build(&frame(&camera, &VideoOptions::default(), &world, &adapter));
        let left = view.left.join("\n");
        assert!(left.contains("E: 4/9"), "{left}");
        assert!(left.contains("P: 17"), "{left}");
        assert!(left.contains("Biome: Plains"), "{left}");
        assert!(left.contains("Looking at: 1 2 3"), "{left}");
        assert!(view.right.contains(&"minecraft:stone".to_owned()));
        assert!(view.right.contains(&"Test GPU".to_owned()));
        assert!(view.right.contains(&"Display: 854x480 (radv)".to_owned()));
    }
}
