// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the block-array mip levels: a one-texel
//! checkerboard seen from far away is smoothed to grey with mips and stays
//! black and white without them (`Mipmap Levels` 4 and 0).
//!
//! Run with
//! `cargo test -p umicraft-render --test offscreen_mipmaps -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_mesh::{Mesh, Vertex, VertexLight};
use umicraft_render::{Camera, Image, Overlay, RenderEnvironment, Renderer};
use umicraft_world::ChunkPos;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;
const TEXELS: u32 = 16;

/// A 16x16 layer of alternating black and white texels.
fn checkerboard() -> Vec<u8> {
    (0..TEXELS * TEXELS)
        .flat_map(|index| {
            let (x, y) = (index % TEXELS, index / TEXELS);
            if (x + y) % 2 == 0 {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 255]
            }
        })
        .collect()
}

/// A small quad far from the camera, inside chunk (0, -4), so each texel
/// covers under a pixel.
fn far_quad() -> Mesh {
    let light = VertexLight::new(15, 0, VertexLight::AO_OPEN, false).pack();
    let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
        position: [x, y, -60.0],
        normal: [0.0, 0.0, 1.0],
        uv,
        layer: 0,
        light,
        tint: umicraft_mesh::tint::WHITE,
    };
    Mesh {
        vertices: vec![
            vertex(4.0, 77.0, [0.0, 1.0]),
            vertex(10.0, 77.0, [1.0, 1.0]),
            vertex(10.0, 83.0, [1.0, 0.0]),
            vertex(4.0, 83.0, [0.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    }
}

/// The spread of the red channel over the middle of the frame, where the
/// quad is: large when texels alias, small when the mip averages them.
fn quad_contrast(levels: u32) -> u8 {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_mipmap_levels(levels);
    renderer
        .set_block_atlas(TEXELS, TEXELS, 1, &checkerboard())
        .expect("atlas");
    renderer.upload_mesh(ChunkPos::new(0, -4), &far_quad());
    let mut camera = Camera::new(Vec3::new(7.0, 80.0, 0.0), 0.0, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer.set_environment(RenderEnvironment::new(0, 6_000, 0.0, 8));
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    let image = renderer.read_pixels().expect("read back");
    let reds = middle_reds(&image);
    reds.iter().max().copied().unwrap_or(0) - reds.iter().min().copied().unwrap_or(0)
}

fn middle_reds(image: &Image) -> Vec<u8> {
    let (cx, cy) = (image.width / 2, image.height / 2);
    (cy - 2..cy + 2)
        .flat_map(|y| (cx - 2..cx + 2).map(move |x| (x, y)))
        .map(|(x, y)| image.rgba[((y * image.width + x) * 4) as usize])
        .collect()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn mip_levels_smooth_a_minified_checkerboard() {
    let with_mips = quad_contrast(4);
    let without = quad_contrast(0);
    assert!(
        without > 100,
        "single-level sampling aliases the checkerboard (spread {without})"
    );
    assert!(
        with_mips < without / 2,
        "mips average it (spread {with_mips} against {without})"
    );
}
