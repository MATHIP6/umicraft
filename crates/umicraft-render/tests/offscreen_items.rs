// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the item icon atlas: a provider's mesh is
//! rendered once through the terrain pipeline and the UI draws it from the
//! atlas.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_items -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
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

/// Two overlapping sprites, red at z = -5 and green at z = +5.
const LAYERED_KEY: u64 = 77;

impl IconProvider for FlatSprite {
    fn build_icon(&mut self, key: u64, mesh: &mut ItemMesh) -> bool {
        if key == LAYERED_KEY {
            for (z, layer) in [(-5.0, 0), (5.0, 1)] {
                let base = mesh.vertices.len() as u32;
                for (x, y) in [(0.0, 0.0), (16.0, 0.0), (16.0, 16.0), (0.0, 16.0)] {
                    mesh.vertices.push(Vertex {
                        position: [x, y, z],
                        normal: [0.0, 0.0, -1.0],
                        uv: [x / 16.0, y / 16.0],
                        layer,
                        light: VertexLight::new(0, 15, 15, false).pack(),
                        tint: tint::WHITE,
                    });
                }
                mesh.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            return true;
        }
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

#[test]
#[ignore = "requires a GPU adapter"]
fn a_glint_item_is_brightened_and_tinted_only_inside_its_silhouette() {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    // An all-white glint texture makes the result independent of the scroll.
    renderer.set_glint_texture(4, 4, &[255; 4 * 4 * 4]);
    renderer.set_glint_time(1.0);
    let camera = Camera::default();
    let overlay = Overlay::new(SIZE, SIZE);
    let mut provider = FlatSprite { requests: 0 };

    let mut plain = DrawList::new();
    plain.item(KEY, Rect::new(16.0, 16.0, 32.0, 32.0), false);
    renderer
        .render_with_ui(&camera, &overlay, &plain, &mut provider)
        .expect("render");
    let icon = pixel(&mut renderer, 32, 32);
    let outside = [(4, 90), (60, 32)].map(|(x, y)| pixel(&mut renderer, x, y));

    let mut glinting = DrawList::new();
    glinting.item(KEY, Rect::new(16.0, 16.0, 32.0, 32.0), true);
    renderer
        .render_with_ui(&camera, &overlay, &glinting, &mut provider)
        .expect("render");
    let glowing = pixel(&mut renderer, 32, 32);
    assert!(glowing[0] > icon[0] + 60, "red {icon:?} -> {glowing:?}");
    assert!(glowing[2] > icon[2] + 100, "blue {icon:?} -> {glowing:?}");
    assert!(glowing[2] > glowing[0], "purplish {glowing:?}");
    assert_eq!(
        [(4, 90), (60, 32)].map(|(x, y)| pixel(&mut renderer, x, y)),
        outside
    );
}

/// Regression: the GUI projection looks along -z, so the larger z is nearer
/// (the first version had it reversed and drew blocks upside down, seen
/// from below).
#[test]
#[ignore = "requires a GPU adapter"]
fn the_larger_z_is_in_front_in_an_icon() {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    let mut list = DrawList::new();
    list.item(LAYERED_KEY, Rect::new(16.0, 16.0, 32.0, 32.0), false);
    renderer
        .render_with_ui(
            &Camera::default(),
            &Overlay::new(SIZE, SIZE),
            &list,
            &mut FlatSprite { requests: 0 },
        )
        .expect("render");
    let inside = pixel(&mut renderer, 32, 32);
    assert!(
        inside[1] > 200 && inside[0] < 60,
        "green must cover red: {inside:?}"
    );
}

/// A lit sprite of `half` extent in the plane `z`, wound
/// counter-clockwise as seen from +z so the back-face cull keeps it.
fn quad(z: f32, half: f32) -> ItemMesh {
    let mut mesh = ItemMesh::default();
    for (x, y) in [(-half, -half), (half, -half), (half, half), (-half, half)] {
        mesh.vertices.push(Vertex {
            position: [x, y, z],
            normal: [0.0, 0.0, 1.0],
            uv: [x / (2.0 * half) + 0.5, y / (2.0 * half) + 0.5],
            layer: 1,
            light: VertexLight::new(0, 15, 15, false).pack(),
            tint: tint::WHITE,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    mesh
}

/// Renders once and returns the centre pixel and two corner pixels.
fn probe(renderer: &mut Renderer) -> [[u8; 3]; 3] {
    renderer
        .render(
            &Camera::new(Vec3::ZERO, 0.0, 0.0),
            &Overlay::new(SIZE, SIZE),
        )
        .expect("render");
    let image = renderer.read_pixels().expect("readback");
    [(SIZE / 2, SIZE / 2), (2, 2), (SIZE - 3, SIZE - 3)].map(|(x, y)| {
        let index = ((y * image.width + x) * 4) as usize;
        [
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
        ]
    })
}

fn assert_glint_only_on_quad(plain: [[u8; 3]; 3], glowing: [[u8; 3]; 3]) {
    let widen = |pixel: [u8; 3]| pixel.map(u32::from);
    let (before, after) = (widen(plain[0]), widen(glowing[0]));
    assert!(after[2] > before[2] + 40, "blue {before:?} -> {after:?}");
    assert!(after[0] > before[0] + 10, "red {before:?} -> {after:?}");
    assert!(after[2] > after[0], "purplish {after:?}");
    assert_eq!(plain[1..], glowing[1..], "outside the quad");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_world_item_glints_only_on_its_visible_surface() {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    renderer.set_glint_texture(4, 4, &[255; 4 * 4 * 4]);
    renderer.set_glint_time(1.0);
    // `probe` looks along -z from the origin.
    let mesh = quad(-3.0, 1.0);
    renderer.set_world_items(&mesh);
    let plain = probe(&mut renderer);
    renderer.set_world_glint(&mesh);
    let glowing = probe(&mut renderer);
    assert_glint_only_on_quad(plain, glowing);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_hand_item_glints_only_on_its_visible_surface() {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    renderer.set_glint_texture(4, 4, &[255; 4 * 4 * 4]);
    renderer.set_glint_time(1.0);
    let mesh = quad(-1.0, 0.4);
    renderer.set_hand_items(&mesh);
    let plain = probe(&mut renderer);
    renderer.set_hand_glint(&mesh);
    let glowing = probe(&mut renderer);
    assert_glint_only_on_quad(plain, glowing);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn glint_meshes_draw_nothing_without_a_glint_texture() {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    renderer.set_block_atlas(2, 2, 2, &atlas()).expect("atlas");
    let mesh = quad(-3.0, 1.0);
    renderer.set_world_items(&mesh);
    let plain = probe(&mut renderer);
    renderer.set_world_glint(&mesh);
    assert_eq!(probe(&mut renderer), plain);
}
