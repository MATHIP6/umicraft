// SPDX-License-Identifier: MIT OR Apache-2.0

//! Public-surface tests for [`umicraft_render::UiPass`].
//!
//! These exercise the frozen `DrawList`/`FontAtlas` contract from the render
//! side without a GPU: vertex order, nested clipping and atlas UV mapping.

use umicraft_render::{fallback_atlas, UiPass};
use umicraft_ui::{DrawList, FontAtlas, Glyph, Rect, TextStyle};

const COLOR: [f32; 4] = [0.2, 0.4, 0.6, 0.8];

/// A 16×8 atlas with two full-height glyphs and a white texel in the last
/// quarter of the bitmap.
fn synthetic_atlas() -> FontAtlas {
    FontAtlas::new(
        16,
        8,
        (0..16 * 8).map(|index| index as u8).collect(),
        vec![
            Glyph {
                character: 'A',
                uv_min: [0.0, 0.0],
                uv_max: [0.5, 1.0],
                advance: 4.0,
            },
            Glyph {
                character: 'B',
                uv_min: [0.5, 0.0],
                uv_max: [1.0, 1.0],
                advance: 6.0,
            },
        ],
        10.0,
        [0.75, 0.5, 1.0, 1.0],
    )
}

fn ndc(x: f32, y: f32) -> [f32; 2] {
    [2.0 * x / 100.0 - 1.0, 1.0 - 2.0 * y / 100.0]
}

fn assert_close(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 1e-5 && (actual[1] - expected[1]).abs() < 1e-5,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn rectangles_text_and_clips_keep_command_order() {
    let mut list = DrawList::new();
    list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0));
    list.rect(Rect::new(10.0, 10.0, 10.0, 10.0), COLOR);
    list.text([2.0, 2.0], "AB", TextStyle::new(1.0, COLOR));
    list.pop_clip();
    list.rect(Rect::new(60.0, 60.0, 10.0, 10.0), COLOR);

    let mut pass = UiPass::new(100, 100);
    pass.rebuild(&list, &synthetic_atlas());

    assert_eq!(pass.vertex_count(), 6 + 12 + 6);
    assert_close(pass.vertices()[0].position, ndc(10.0, 10.0));
    assert_close(pass.vertices()[6].position, ndc(2.0, 2.0));
    assert_eq!(pass.vertices()[6].uv, [0.0, 0.0]);
    assert_close(pass.vertices()[12].position, ndc(6.0, 2.0));
    assert_eq!(pass.vertices()[12].uv, [0.5, 0.0]);
    assert_close(pass.vertices()[18].position, ndc(60.0, 60.0));
}

#[test]
fn nested_clips_intersect_and_an_empty_intersection_draws_nothing() {
    let mut list = DrawList::new();
    list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0));
    list.push_clip(Rect::new(25.0, 25.0, 50.0, 50.0));
    list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);
    list.pop_clip();
    list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);
    list.pop_clip();
    // Same commands again with clips that do not overlap: nothing may be
    // emitted between the second push and its pop.
    list.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
    list.push_clip(Rect::new(20.0, 20.0, 10.0, 10.0));
    list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);
    list.text([0.0, 0.0], "A", TextStyle::new(1.0, COLOR));
    list.pop_clip();
    list.pop_clip();

    let mut pass = UiPass::new(100, 100);
    pass.rebuild(&list, &synthetic_atlas());

    assert_eq!(pass.vertex_count(), 12);
    assert_close(pass.vertices()[0].position, [-0.5, 0.5]);
    assert_close(pass.vertices()[2].position, [0.0, 0.0]);
    assert_close(pass.vertices()[6].position, [-1.0, 1.0]);
    assert_close(pass.vertices()[8].position, [0.0, 0.0]);
}

#[test]
fn white_texel_and_clipped_glyph_uvs_come_from_the_atlas() {
    let atlas = synthetic_atlas();
    let mut list = DrawList::new();
    list.rect(Rect::new(0.0, 0.0, 10.0, 10.0), COLOR);
    list.push_clip(Rect::new(2.0, 0.0, 4.0, 8.0));
    list.text([0.0, 0.0], "A", TextStyle::new(1.0, COLOR));

    let mut pass = UiPass::new(100, 100);
    pass.rebuild(&list, &atlas);

    assert_eq!(pass.vertex_count(), 12);
    // Rectangles sample the centre of the white UV rectangle.
    for vertex in &pass.vertices()[..6] {
        assert_eq!(vertex.uv, [0.875, 0.75]);
    }
    // The 8×8 glyph is clipped to x = 2..6, so its UVs keep the matching
    // half-open portion of the 0.0..0.5 cell.
    assert_close(pass.vertices()[6].uv, [0.125, 0.0]);
    assert_close(pass.vertices()[7].uv, [0.375, 0.0]);
    assert_close(pass.vertices()[8].uv, [0.375, 1.0]);
    assert_close(pass.vertices()[11].uv, [0.125, 1.0]);
}

#[test]
fn the_public_fallback_atlas_matches_the_hud_metrics() {
    let atlas = fallback_atlas();
    assert_eq!((atlas.width, atlas.height), (32, 48));
    assert_eq!(atlas.pixels.len(), 32 * 48);
    assert_eq!(atlas.line_height, 8.0);
    let upper = atlas.glyph('A').expect("fallback has A");
    let lower = atlas.glyph('a').expect("fallback maps lowercase");
    assert_eq!(upper.uv_min, lower.uv_min);
    assert_eq!(upper.advance, 4.0);
    assert!(atlas.white_uv[2] > atlas.white_uv[0]);
    assert!(atlas.white_uv[3] > atlas.white_uv[1]);
}

#[test]
fn an_empty_list_produces_no_vertices() {
    let mut pass = UiPass::new(100, 100);
    pass.rebuild(&DrawList::new(), &synthetic_atlas());
    assert!(pass.is_empty());
    assert_eq!(pass.vertex_count(), 0);
    assert!(pass.vertices().is_empty());
}
