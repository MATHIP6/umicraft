// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the name tags of the world pass: the `tested` part
//! of a tag is hidden by terrain in front of its anchor and the `always` part
//! is not (MCP `Render.renderLivingLabel`).
//!
//! Run with
//! `cargo test -p umicraft-render --test offscreen_nametag -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_mesh::{Mesh, Vertex, VertexLight};
use umicraft_render::{Camera, Nametag, Overlay, RenderEnvironment, Renderer};
use umicraft_ui::{DrawList, Rect};
use umicraft_world::ChunkPos;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;
const WALL_Z: f32 = -5.0;
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// A wall facing the camera at `WALL_Z`, in the chunk (0, -1).
fn wall() -> Mesh {
    let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
        position: [x, y, WALL_Z],
        normal: [0.0, 0.0, 1.0],
        uv,
        layer: 0,
        light: VertexLight::new(15, 0, VertexLight::AO_OPEN, false).pack(),
        tint: umicraft_mesh::tint::WHITE,
    };
    Mesh {
        vertices: vec![
            vertex(0.0, 70.0, [0.0, 1.0]),
            vertex(16.0, 70.0, [1.0, 1.0]),
            vertex(16.0, 90.0, [1.0, 0.0]),
            vertex(0.0, 90.0, [0.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    }
}

fn red_box() -> DrawList {
    let mut list = DrawList::new();
    list.rect(Rect::new(54.0, 38.0, 20.0, 20.0), RED);
    list
}

/// Renders the wall and one tag whose anchor is at `z`; true if the middle of
/// the frame is red.
fn is_red(z: f32, always: DrawList, tested: DrawList) -> bool {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.upload_mesh(ChunkPos::new(0, -1), &wall());
    let mut camera = Camera::new(Vec3::new(8.0, 80.0, 0.0), 0.0, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    let clip = camera.view_projection() * Vec3::new(8.0, 80.0, z).extend(1.0);
    let tag = Nametag {
        always,
        tested,
        depth: clip.z / clip.w,
    };
    renderer.set_nametags(&[tag], (WIDTH, HEIGHT));
    renderer.set_environment(RenderEnvironment::new(0, 6_000, 0.0, 8));
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    let image = renderer.read_pixels().expect("read back the frame");
    let index = (((HEIGHT / 2) * image.width + WIDTH / 2) * 4) as usize;
    let pixel = &image.rgba[index..index + 4];
    pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 60
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_tested_part_of_a_tag_is_hidden_by_terrain_in_front_of_it() {
    assert!(
        is_red(-3.0, DrawList::new(), red_box()),
        "in front of the wall"
    );
    assert!(
        !is_red(-10.0, DrawList::new(), red_box()),
        "behind the wall"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_always_part_of_a_tag_shows_through_terrain() {
    assert!(is_red(-10.0, red_box(), DrawList::new()), "behind the wall");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_wall_covers_the_middle_of_the_frame() {
    assert!(!is_red(-3.0, DrawList::new(), DrawList::new()));
}
