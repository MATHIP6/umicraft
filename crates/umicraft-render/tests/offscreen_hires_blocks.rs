// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the second texture array, which keeps the textures
//! larger than the layers of the block array at their own size (a resource
//! pack's 512 pixel ender pearl).
//!
//! Run with `cargo test -p umicraft-render --test offscreen_hires_blocks -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::{tint, Vertex, VertexLight};
use umicraft_render::{Camera, Overlay, Renderer};

const SIZE: u32 = 96;

/// A 2x2, 2-layer array: layer 0 red, layer 1 blue. Layer 1 is the shrunk copy
/// of the large texture.
fn atlas() -> Vec<u8> {
    let mut pixels = [255, 0, 0, 255].repeat(4);
    pixels.extend([0, 0, 255, 255].repeat(4));
    pixels
}

/// A 4x4 large texture: its left half green, its right half yellow, finer than
/// the 2x2 copy could ever show.
fn large() -> Vec<u8> {
    (0..16)
        .flat_map(|texel| {
            if texel % 4 < 2 {
                [0, 255, 0, 255]
            } else {
                [255, 255, 0, 255]
            }
        })
        .collect()
}

/// A lit quad of `half` extent at `z` sampling `layer`, wound so the back-face
/// cull keeps it.
fn quad(layer: u32) -> ItemMesh {
    let half = 1.0;
    let mut mesh = ItemMesh::default();
    for (x, y) in [(-half, -half), (half, -half), (half, half), (-half, half)] {
        mesh.vertices.push(Vertex {
            position: [x, y, -3.0],
            normal: [0.0, 0.0, 1.0],
            uv: [x / (2.0 * half) + 0.5, y / (2.0 * half) + 0.5],
            layer,
            light: VertexLight::new(0, 15, 15, false).pack(),
            tint: tint::WHITE,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    mesh
}

/// Renders the quad on `layer` and returns the pixels left and right of the
/// centre, both inside the quad.
fn probe(renderer: &mut Renderer, layer: u32) -> [[u8; 3]; 2] {
    renderer.set_world_items(&quad(layer));
    renderer
        .render(
            &Camera::new(Vec3::ZERO, 0.0, 0.0),
            &Overlay::new(SIZE, SIZE),
        )
        .expect("render");
    let image = renderer.read_pixels().expect("readback");
    [SIZE / 2 - 8, SIZE / 2 + 8].map(|x| {
        let index = ((SIZE / 2 * image.width + x) * 4) as usize;
        [
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
        ]
    })
}

fn renderer() -> Renderer {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    renderer
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_large_texture_replaces_the_shrunk_copy_and_keeps_its_detail() {
    let mut renderer = renderer();
    let [left, right] = probe(&mut renderer, 1);
    assert!(
        left[2] > 200 && left[0] < 60,
        "the blue copy first: {left:?}"
    );
    assert_eq!(left, right, "the copy is one flat colour");

    renderer
        .set_hires_blocks(4, &[(1, &large())])
        .expect("large texture");
    let [left, right] = probe(&mut renderer, 1);
    assert!(left[1] > 200 && left[0] < 60, "green half: {left:?}");
    assert!(right[0] > 200 && right[1] > 200, "yellow half: {right:?}");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_layer_without_a_large_copy_keeps_the_block_array() {
    let mut renderer = renderer();
    renderer
        .set_hires_blocks(4, &[(1, &large())])
        .expect("large texture");
    let [left, right] = probe(&mut renderer, 0);
    assert!(left[0] > 200 && left[2] < 60, "layer 0 stays red: {left:?}");
    assert_eq!(left, right);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_new_block_array_forgets_the_large_textures_of_the_old_one() {
    let mut renderer = renderer();
    renderer
        .set_hires_blocks(4, &[(1, &large())])
        .expect("large texture");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    let [left, right] = probe(&mut renderer, 1);
    assert!(left[2] > 200 && left[0] < 60, "blue again: {left:?}");
    assert_eq!(left, right);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_wrong_pixel_count_is_refused_and_changes_nothing() {
    let mut renderer = renderer();
    assert!(renderer.set_hires_blocks(4, &[(1, &[0; 10])]).is_err());
    let [left, _] = probe(&mut renderer, 1);
    assert!(
        left[2] > 200 && left[0] < 60,
        "still the blue copy: {left:?}"
    );
}
