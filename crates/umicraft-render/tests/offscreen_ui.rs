// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests: a synthetic UI draw list renders on top of terrain
//! and the M3 debug overlay without breaking either of them, and a synthetic
//! block texture array replaces the procedural fallback with alpha cutout.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_ui -- --ignored`
//! on a machine with a wgpu adapter.

use umicraft_mesh::{Mesh, Vertex, VertexLight};
use umicraft_render::{
    fallback_atlas, BlockAtlasError, Camera, FontAtlasError, Image, Overlay, Renderer,
};
use umicraft_ui::{DrawList, Rect, TextStyle};
use umicraft_world::ChunkPos;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;

/// One large quad in front of the default camera, with the fallback layer 0
/// texture. This stands in for a chunk mesh so the test needs no world data.
fn terrain_quad() -> Mesh {
    let normal = [0.0, 0.0, 1.0];
    let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
        position: [x, y, -5.0],
        normal,
        uv,
        layer: 0,
        light: VertexLight::DAY.pack(),
    };
    Mesh {
        vertices: vec![
            vertex(-2.0, 78.5, [0.0, 1.0]),
            vertex(2.0, 78.5, [1.0, 1.0]),
            vertex(2.0, 81.5, [1.0, 0.0]),
            vertex(-2.0, 81.5, [0.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    }
}

fn center_pixel(image: &Image) -> [u8; 3] {
    let index = ((image.height / 2 * image.width + image.width / 2) * 4) as usize;
    [
        image.rgba[index],
        image.rgba[index + 1],
        image.rgba[index + 2],
    ]
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    [
        image.rgba[index],
        image.rgba[index + 1],
        image.rgba[index + 2],
        image.rgba[index + 3],
    ]
}

/// Layer-major RGBA8 for a 2×2, 3-layer synthetic atlas (48 bytes). Layer 0
/// is transparent on its left texel column and opaque red on the right; layers
/// 1 and 2 are opaque green and blue.
fn synthetic_atlas() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(48);
    for _ in 0..2 {
        pixels.extend_from_slice(&[255, 0, 0, 0]);
        pixels.extend_from_slice(&[255, 0, 0, 255]);
    }
    pixels.extend_from_slice(&[0, 255, 0, 255].repeat(4));
    pixels.extend_from_slice(&[0, 0, 255, 255].repeat(4));
    pixels
}

#[test]
#[ignore = "requires a GPU adapter"]
fn ui_draw_list_renders_over_terrain_and_the_debug_overlay() {
    let (width, height) = (WIDTH, HEIGHT);
    let mut renderer = Renderer::new_offscreen(width, height).expect("offscreen renderer");
    renderer.upload_mesh(ChunkPos::new(0, 0), &terrain_quad());
    let mut camera = Camera::default();
    camera.set_aspect(width, height);

    // Baseline: terrain only.
    let empty = Overlay::new(width, height);
    renderer.render(&camera, &empty).expect("render terrain");
    let terrain_only = renderer.read_pixels().expect("read terrain");
    let sky = &terrain_only.rgba[..3];
    assert!(
        center_pixel(&terrain_only) != sky,
        "the synthetic quad must cover the centre pixel"
    );

    // M3 path: the debug overlay still draws through Overlay::text.
    let mut hud = Overlay::new(width, height);
    hud.rect(0.0, 0.0, width as f32, 12.0, [0.0, 0.0, 0.0, 1.0]);
    hud.text(2.0, 2.0, 1.0, [1.0; 4], "HUD");
    renderer.render(&camera, &hud).expect("render hud");
    let with_hud = renderer.read_pixels().expect("read hud");
    assert_ne!(with_hud.rgba, terrain_only.rgba, "the HUD must be visible");

    // M4 path: upload the fallback atlas and draw a synthetic UI frame.
    let atlas = fallback_atlas();
    renderer
        .set_font_atlas(&atlas)
        .expect("upload fallback atlas");
    let mut list = DrawList::new();
    list.rect(Rect::new(20.0, 30.0, 60.0, 40.0), [0.8, 0.1, 0.1, 1.0]);
    list.push_clip(Rect::new(20.0, 30.0, 60.0, 20.0));
    list.text([24.0, 34.0], "UI PASS", TextStyle::new(1.5, [1.0; 4]));
    list.pop_clip();
    renderer
        .render_with_ui(&camera, &hud, &list)
        .expect("render ui");
    let with_ui = renderer.read_pixels().expect("read ui");
    assert_ne!(
        with_ui.rgba, with_hud.rgba,
        "the UI pass must change the screenshot"
    );

    // A malformed atlas is rejected before it reaches wgpu validation.
    let mut broken = fallback_atlas();
    broken.pixels.pop();
    assert!(matches!(
        renderer.set_font_atlas(&broken),
        Err(FontAtlasError::PixelCount { .. })
    ));

    // Rendering without the UI must not keep stale UI geometry.
    renderer.render(&camera, &hud).expect("render hud again");
    let without_ui = renderer.read_pixels().expect("read hud again");
    assert_eq!(without_ui.rgba, with_hud.rgba);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn block_atlas_upload_replaces_the_terrain_array() {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.upload_mesh(ChunkPos::new(0, 0), &terrain_quad());
    let mut camera = Camera::default();
    camera.set_aspect(WIDTH, HEIGHT);
    let empty = Overlay::new(WIDTH, HEIGHT);

    // A 2×2, 3-layer array proves the real size and layer count are used
    // rather than the 19-layer, 16×16 fallback constants.
    renderer
        .set_block_atlas(2, 2, 3, &synthetic_atlas())
        .expect("upload synthetic atlas");
    renderer.render(&camera, &empty).expect("render atlas");
    let image = renderer.read_pixels().expect("read atlas");
    assert_eq!((image.width, image.height), (WIDTH, HEIGHT));
    assert_eq!(image.rgba.len(), (WIDTH * HEIGHT * 4) as usize);

    // The quad spans NDC x = ±0.43 at z = −5, so its texture columns land
    // near x = 48 (left texel, transparent) and x = 80 (right texel, red),
    // with the quad edge around x = 36.
    let sky = pixel(&image, 0, 0);
    let opaque = pixel(&image, 80, HEIGHT / 2);
    assert!(
        opaque[0] > 150 && opaque[1] < 60 && opaque[2] < 60,
        "the opaque half must sample the red layer: {opaque:?}"
    );
    assert_eq!(
        pixel(&image, 48, HEIGHT / 2),
        sky,
        "alpha cutout must discard the transparent half"
    );

    // A second upload replaces the array instead of appending to it.
    renderer
        .set_block_atlas(2, 2, 1, &[0u8, 0, 255, 255].repeat(4))
        .expect("replace the atlas");
    renderer
        .render(&camera, &empty)
        .expect("render replacement");
    let replaced = renderer.read_pixels().expect("read replacement");
    let center = pixel(&replaced, WIDTH / 2, HEIGHT / 2);
    assert!(
        center[2] > 150 && center[0] < 60 && center[1] < 60,
        "the replacement layer must be sampled: {center:?}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_rejected_block_atlas_keeps_the_fallback_array_bound() {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.upload_mesh(ChunkPos::new(0, 0), &terrain_quad());
    let mut camera = Camera::default();
    camera.set_aspect(WIDTH, HEIGHT);
    let empty = Overlay::new(WIDTH, HEIGHT);

    renderer.render(&camera, &empty).expect("render fallback");
    let fallback = renderer.read_pixels().expect("read fallback");
    assert_ne!(
        pixel(&fallback, WIDTH / 2, HEIGHT / 2),
        pixel(&fallback, 0, 0),
        "the fallback layer 0 must cover the centre"
    );

    // A short buffer and an empty array are rejected before the GPU is touched.
    assert_eq!(
        renderer.set_block_atlas(2, 2, 3, &[0; 3]),
        Err(BlockAtlasError::PixelCount {
            width: 2,
            height: 2,
            layers: 3,
            expected: 48,
            actual: 3,
        })
    );
    assert_eq!(
        renderer.set_block_atlas(2, 2, 0, &[]),
        Err(BlockAtlasError::EmptySize)
    );

    // The fallback array and its bind group are still bound: the frame is
    // pixel-identical to the baseline render.
    renderer
        .render(&camera, &empty)
        .expect("render fallback again");
    let after = renderer.read_pixels().expect("read fallback again");
    assert_eq!(
        after.rgba, fallback.rgba,
        "a rejected upload must leave the fallback frame untouched"
    );
}
