// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the item icon atlas: a provider's mesh is
//! rendered once through the terrain pipeline and the UI draws it from the
//! atlas.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_items -- --ignored`
//! on a machine with a wgpu adapter.

use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::{tint, Vertex, VertexLight};
use umicraft_render::{Camera, IconProvider, Overlay, Renderer};
use umicraft_ui::{DrawList, Rect};

const SIZE: u32 = 96;
const KEY: u64 = 42;

/// Hands out one flat sprite on layer 1 filling the whole 16x16 icon box,
/// and counts how often it was asked.
struct FlatSprite {
    requests: u32,
}

impl IconProvider for FlatSprite {
    fn build_icon(&mut self, key: u64, mesh: &mut ItemMesh) -> bool {
        if key != KEY {
            return false;
        }
        self.requests += 1;
        let vertex = |x: f32, y: f32| Vertex {
            position: [x, y, 0.0],
            normal: [0.0, 0.0, -1.0],
            uv: [x / 16.0, y / 16.0],
            layer: 1,
            light: VertexLight::new(0, 15, 15, false).pack(),
            tint: tint::WHITE,
        };
        mesh.vertices.extend([
            vertex(0.0, 0.0),
            vertex(16.0, 0.0),
            vertex(16.0, 16.0),
            vertex(0.0, 16.0),
        ]);
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        true
    }
}

/// A 2x2, 2-layer atlas: layer 0 red, layer 1 opaque green.
fn atlas() -> Vec<u8> {
    let mut pixels = [255, 0, 0, 255].repeat(4);
    pixels.extend([0, 255, 0, 255].repeat(4));
    pixels
}

fn pixel(renderer: &mut Renderer, x: u32, y: u32) -> [u8; 3] {
    let image = renderer.read_pixels().expect("readback");
    let index = ((y * image.width + x) * 4) as usize;
    [
        image.rgba[index],
        image.rgba[index + 1],
        image.rgba[index + 2],
    ]
}

#[test]
#[ignore = "requires a GPU adapter"]
fn an_item_command_draws_the_provided_icon_once() {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    let camera = Camera::default();
    let overlay = Overlay::new(SIZE, SIZE);
    let mut list = DrawList::new();
    list.item(KEY, Rect::new(16.0, 16.0, 32.0, 32.0), false);
    list.item(99, Rect::new(56.0, 56.0, 32.0, 32.0), false);
    let mut provider = FlatSprite { requests: 0 };

    renderer
        .render_with_ui(&camera, &overlay, &list, &mut provider)
        .expect("render");
    let inside = pixel(&mut renderer, 32, 32);
    let outside = pixel(&mut renderer, 4, 90);
    assert!(inside[1] > 200 && inside[0] < 60, "icon pixel {inside:?}");
    assert_ne!(inside, outside);
    // The unknown key draws nothing: its area keeps the sky colour.
    assert_eq!(pixel(&mut renderer, 72, 72), outside);

    // A second frame reuses the atlas cell instead of rebuilding the mesh.
    renderer
        .render_with_ui(&camera, &overlay, &list, &mut provider)
        .expect("render");
    assert_eq!(provider.requests, 1);
    assert_eq!(pixel(&mut renderer, 32, 32), inside);
}
