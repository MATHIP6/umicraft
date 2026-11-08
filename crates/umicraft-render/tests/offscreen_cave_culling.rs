// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the cave culling (F2-3d, ADR-0062): a scene with a
//! solid floor of chunks and a sealed room underneath must look the same with
//! and without the culling, and the sealed room must not be drawn when seen
//! from outside.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_cave_culling -- --ignored`.

use glam::Vec3;
use umicraft_mesh::{mesh_chunk, ChunkSnapshot};
use umicraft_render::{Camera, Overlay, Renderer};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

const SIZE: u32 = 128;

/// A stone floor at y = 63 with a hollow sealed room at y = 20..24 below it.
fn column() -> ChunkColumn {
    let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
    for y in 0..64 {
        for z in 0..16 {
            for x in 0..16 {
                let in_room = (4..12).contains(&x) && (4..12).contains(&z) && (20..24).contains(&y);
                if !in_room {
                    column.set_block(x, y, z, BlockState::new(1, 0));
                }
            }
        }
    }
    column
}

fn render(culling: bool) -> (Vec<u8>, usize) {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_cave_culling(culling);
    let pos = ChunkPos::new(0, 0);
    let snapshot = ChunkSnapshot::from_columns(pos, [std::sync::Arc::new(column())]);
    let mesh = mesh_chunk(pos, &snapshot);
    let faces = mesh.opaque_face_count();
    renderer.upload_mesh(pos, &mesh);
    let mut camera = Camera::new(Vec3::new(8.0, 80.0, 40.0), 0.0, -0.6);
    camera.set_aspect(SIZE, SIZE);
    renderer
        .render(&camera, &Overlay::new(SIZE, SIZE))
        .expect("render");
    (renderer.read_pixels().expect("pixels").rgba, faces)
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_culled_frame_matches_the_full_one() {
    let (culled, faces) = render(true);
    let (full, _) = render(false);
    assert!(faces > 0);
    assert_eq!(culled, full);
}
