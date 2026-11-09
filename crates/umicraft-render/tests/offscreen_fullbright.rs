// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the Fullbright mod: a wall that no light reaches is
//! nearly black at night, and lit at full strength with the mod on.
//!
//! Run with
//! `cargo test -p umicraft-render --test offscreen_fullbright -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_mesh::{Mesh, Vertex, VertexLight};
use umicraft_render::{Camera, Overlay, RenderEnvironment, Renderer};
use umicraft_world::ChunkPos;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

/// A wall facing the camera, with no block light and no sky light.
fn dark_wall() -> Mesh {
    let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
        position: [x, y, -5.0],
        normal: [0.0, 0.0, 1.0],
        uv,
        layer: 0,
        light: VertexLight::new(0, 0, VertexLight::AO_OPEN, false).pack(),
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

/// The mean brightness of the middle of the frame, at midnight.
fn brightness(fullbright: bool) -> f64 {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.upload_mesh(ChunkPos::new(0, -1), &dark_wall());
    renderer.set_fullbright(fullbright);
    let mut camera = Camera::new(Vec3::new(8.0, 80.0, 0.0), 0.0, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer.set_environment(RenderEnvironment::new(0, 18_000, 0.0, 8));
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    let image = renderer.read_pixels().expect("read back the frame");
    let mut sum = 0.0;
    let mut count = 0.0;
    for y in HEIGHT / 3..2 * HEIGHT / 3 {
        for x in WIDTH / 3..2 * WIDTH / 3 {
            let at = ((y * image.width + x) * 4) as usize;
            sum += f64::from(image.rgba[at])
                + f64::from(image.rgba[at + 1])
                + f64::from(image.rgba[at + 2]);
            count += 3.0;
        }
    }
    sum / count
}

#[test]
#[ignore = "requires a GPU adapter"]
fn fullbright_lights_a_wall_no_light_reaches() {
    let dark = brightness(false);
    let lit = brightness(true);
    assert!(
        lit > dark * 2.0 && lit > 60.0,
        "dark {dark:.1}, fullbright {lit:.1}"
    );
}
