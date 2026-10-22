// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test: a synthetic UI draw list renders on top of terrain
//! and the M3 debug overlay without breaking either of them.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_ui -- --ignored`
//! on a machine with a wgpu adapter.

use umicraft_mesh::{Mesh, Vertex};
use umicraft_render::{fallback_atlas, Camera, FontAtlasError, Overlay, Renderer};
use umicraft_ui::{DrawList, Rect, TextStyle};
use umicraft_world::ChunkPos;

/// One large quad in front of the default camera, with the fallback layer 0
/// texture. This stands in for a chunk mesh so the test needs no world data.
fn terrain_quad() -> Mesh {
    let normal = [0.0, 0.0, 1.0];
    let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
        position: [x, y, -5.0],
        normal,
        uv,
        layer: 0,
    };
    Mesh {
        vertices: vec![
            vertex(-2.0, 78.5, [0.0, 1.0]),
            vertex(2.0, 78.5, [1.0, 1.0]),
            vertex(2.0, 81.5, [1.0, 0.0]),
            vertex(-2.0, 81.5, [0.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

fn center_pixel(image: &umicraft_render::Image) -> [u8; 3] {
    let index = ((image.height / 2 * image.width + image.width / 2) * 4) as usize;
    [
        image.rgba[index],
        image.rgba[index + 1],
        image.rgba[index + 2],
    ]
}

#[test]
#[ignore = "requires a GPU adapter"]
fn ui_draw_list_renders_over_terrain_and_the_debug_overlay() {
    let (width, height) = (128, 96);
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
