// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conversion of an [`umicraft_ui::DrawList`] into overlay vertices.
//!
//! The UI crate builds a backend-agnostic draw list of rectangles, text runs
//! and clip scopes (ADR-0006). This module turns that list into the same
//! [`OverlayVertex`] stream the debug overlay already uses, so both share one
//! pipeline, one vertex format and one font texture slot.
//!
//! # Clipping
//!
//! Clipping is performed **on the CPU** while the vertices are generated
//! rather than with `wgpu::RenderPass::set_scissor_rect`. A scissor rectangle
//! is part of the render-pass state, so supporting ordered, nested scopes
//! would require splitting the frame into one draw call per scissor change and
//! rebuilding the pass state around each one. CPU clipping instead:
//!
//! - keeps the whole UI in a single draw call, in command order;
//! - intersects nested scopes for free and can represent an empty
//!   intersection by emitting no geometry at all;
//! - is unit-testable without a GPU, which the M4 acceptance suite needs.
//!
//! Glyph quads that are only partially visible have their UVs adjusted
//! proportionally, so a clipped glyph still samples the right part of its
//! cell. Rectangles sample the atlas white texel, whose UV is the same for all
//! four corners.
//!
//! The base clip is the viewport: geometry outside the render target is never
//! emitted, exactly as a scissor rectangle covering the target would do.

use thiserror::Error;
use umicraft_ui::{DrawCommand, DrawList, FontAtlas, Glyph, Rect};

use crate::font;
use crate::overlay::{Overlay, OverlayVertex};

/// Errors from validating a [`FontAtlas`] before uploading it as a texture.
#[derive(Debug, Error, PartialEq)]
pub enum FontAtlasError {
    /// One of the atlas dimensions is zero.
    #[error("font atlas must have a non-zero width and height")]
    EmptySize,
    /// The pixel count of the atlas does not match `width × height`.
    #[error("font atlas has {actual} pixels, expected {expected} for {width}x{height}")]
    PixelCount {
        /// Declared atlas width.
        width: u32,
        /// Declared atlas height.
        height: u32,
        /// `width × height`, the expected pixel count.
        expected: usize,
        /// Length of [`FontAtlas::pixels`].
        actual: usize,
    },
    /// `width × height` does not fit in a `usize`.
    #[error("font atlas size {width}x{height} overflows a pixel count")]
    SizeOverflow {
        /// Declared atlas width.
        width: u32,
        /// Declared atlas height.
        height: u32,
    },
    /// The atlas is larger than the device's maximum 2D texture dimension.
    #[error("font atlas {width}x{height} exceeds the device limit of {max} texels")]
    TooLarge {
        /// Declared atlas width.
        width: u32,
        /// Declared atlas height.
        height: u32,
        /// `wgpu::Limits::max_texture_dimension_2d` of the device.
        max: u32,
    },
    /// The white UV rectangle is not a finite, normalised, non-empty rectangle.
    #[error("font atlas white UV {0:?} is not a normalised rectangle")]
    InvalidWhiteUv([f32; 4]),
}

/// Builds the original 3×5 fallback font as a [`FontAtlas`].
///
/// The atlas is the one committed to the repository (spec §3): 8 columns ×
/// 8 rows of 4×6 cells, with slot 63 solid white for rectangles. Its metrics
/// match [`Overlay::text`], so the M3 HUD keeps its exact glyph boxes, advance
/// and line height when it samples this atlas.
///
/// Printable ASCII is covered; lowercase characters sample their uppercase
/// cell, as the built-in font has no lowercase forms. Newlines are layout,
/// not glyphs, and are handled by the draw pass.
#[must_use]
pub fn fallback_atlas() -> FontAtlas {
    let mut glyphs = Vec::with_capacity(0x7e - 0x20 + 1);
    for code in 0x20_u8..=0x7e {
        let character = char::from(code);
        let (uv_min, uv_max) = font::cell_uv(font::slot(character));
        glyphs.push(Glyph {
            character,
            uv_min,
            uv_max,
            advance: font::CELL_WIDTH as f32,
        });
    }
    let (white_min, white_max) = font::cell_uv(font::WHITE_SLOT);
    FontAtlas::new(
        font::ATLAS_WIDTH,
        font::ATLAS_HEIGHT,
        font::atlas(),
        glyphs,
        (font::CELL_HEIGHT + 2) as f32,
        [white_min[0], white_min[1], white_max[0], white_max[1]],
    )
}

/// Checks the invariants needed to upload an atlas as an R8 texture.
///
/// The renderer calls this on every externally supplied atlas so a malformed
/// PNG cannot reach `wgpu` validation or an out-of-bounds pixel read.
///
/// # Errors
///
/// Returns the matching [`FontAtlasError`] when a dimension is zero, the
/// pixel buffer length is wrong, the size overflows or exceeds the device
/// limit, or the white UV rectangle is not normalised.
pub(crate) fn validate_font_atlas(
    atlas: &FontAtlas,
    max_texture_dimension: u32,
) -> Result<(), FontAtlasError> {
    if atlas.width == 0 || atlas.height == 0 {
        return Err(FontAtlasError::EmptySize);
    }
    let expected = usize::try_from(atlas.width)
        .ok()
        .and_then(|width| {
            usize::try_from(atlas.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(FontAtlasError::SizeOverflow {
            width: atlas.width,
            height: atlas.height,
        })?;
    if atlas.pixels.len() != expected {
        return Err(FontAtlasError::PixelCount {
            width: atlas.width,
            height: atlas.height,
            expected,
            actual: atlas.pixels.len(),
        });
    }
    if atlas.width > max_texture_dimension || atlas.height > max_texture_dimension {
        return Err(FontAtlasError::TooLarge {
            width: atlas.width,
            height: atlas.height,
            max: max_texture_dimension,
        });
    }
    let [u_min, v_min, u_max, v_max] = atlas.white_uv;
    let normalised = [u_min, v_min, u_max, v_max]
        .iter()
        .all(|value| value.is_finite())
        && u_min >= 0.0
        && v_min >= 0.0
        && u_max <= 1.0
        && v_max <= 1.0
        && u_max > u_min
        && v_max > v_min;
    if !normalised {
        return Err(FontAtlasError::InvalidWhiteUv(atlas.white_uv));
    }
    Ok(())
}

/// Turns one [`DrawList`] into overlay vertices.
///
/// The pass owns a reusable vertex buffer: [`UiPass::rebuild`] clears it and
/// walks the draw list in command order, so a steady-state frame performs no
/// allocation. The resulting vertices are in the same NDC space and use the
/// same [`OverlayVertex`] layout as [`Overlay`], and therefore draw with the
/// existing overlay pipeline.
///
/// # Example
///
/// ```
/// use umicraft_render::{fallback_atlas, UiPass};
/// use umicraft_ui::{DrawList, Rect};
///
/// let mut list = DrawList::new();
/// list.rect(Rect::new(0.0, 0.0, 10.0, 10.0), [1.0, 0.0, 0.0, 1.0]);
///
/// let mut pass = UiPass::new(64, 64);
/// pass.rebuild(&list, &fallback_atlas());
/// assert_eq!(pass.vertex_count(), 6);
/// ```
#[derive(Debug, Clone)]
pub struct UiPass {
    width: f32,
    height: f32,
    overlay: Overlay,
    clip: Option<Rect>,
    clip_stack: Vec<Option<Rect>>,
}

impl UiPass {
    /// Creates an empty UI pass for a `width`×`height` physical-pixel target.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let pixel_width = width.max(1);
        let pixel_height = height.max(1);
        let width = pixel_width as f32;
        let height = pixel_height as f32;
        Self {
            width,
            height,
            overlay: Overlay::new(pixel_width, pixel_height),
            clip: Some(Rect::new(0.0, 0.0, width, height)),
            clip_stack: Vec::new(),
        }
    }

    /// Updates the viewport size and resets the clip to the new viewport.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as f32;
        self.height = height.max(1) as f32;
        self.overlay.resize(width, height);
        self.clip = Some(Rect::new(0.0, 0.0, self.width, self.height));
        self.clip_stack.clear();
    }

    /// Drops all vertices and clip scopes.
    pub fn clear(&mut self) {
        self.overlay.clear();
        self.clip = Some(Rect::new(0.0, 0.0, self.width, self.height));
        self.clip_stack.clear();
    }

    /// True when the last rebuild produced no geometry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overlay.is_empty()
    }

    /// Number of vertices produced by the last rebuild.
    #[must_use]
    pub fn vertex_count(&self) -> u32 {
        self.overlay.vertex_count()
    }

    /// Borrows the vertices produced by the last rebuild.
    #[must_use]
    pub fn vertices(&self) -> &[OverlayVertex] {
        self.overlay.vertices()
    }

    /// Rebuilds the vertex list from `draw_list` in command order.
    ///
    /// Rectangles are filled with the atlas white texel; text runs use the
    /// glyph UVs and advances of `atlas`. A character the atlas does not know
    /// is skipped without advancing, matching [`FontAtlas::advance`] (the UI
    /// crate measures text with the same atlas, so the layout stays
    /// consistent). `PushClip` intersects the current scope with its
    /// rectangle, `PopClip` restores the previous one, and an empty
    /// intersection suppresses every command until the scope is popped.
    pub fn rebuild(&mut self, draw_list: &DrawList, atlas: &FontAtlas) {
        self.clear();
        for command in draw_list.commands() {
            match command {
                DrawCommand::Rect { rect, color } => self.push_rect(*rect, *color, atlas),
                DrawCommand::Text {
                    position,
                    text,
                    style,
                } => self.push_text(*position, text, style.scale, style.color, atlas),
                DrawCommand::PushClip { rect } => self.push_clip(*rect),
                DrawCommand::PopClip => self.pop_clip(),
            }
        }
    }

    fn push_rect(&mut self, rect: Rect, color: [f32; 4], atlas: &FontAtlas) {
        let (Some(visible), Some(clip)) = (drawable(rect), self.clip) else {
            return;
        };
        let Some(visible) = visible.intersect(clip) else {
            return;
        };
        let uv = white_texel_uv(atlas);
        self.overlay.push_quad(
            [visible.x, visible.y, visible.width, visible.height],
            uv,
            uv,
            color,
        );
    }

    fn push_text(
        &mut self,
        position: [f32; 2],
        text: &str,
        scale: f32,
        color: [f32; 4],
        atlas: &FontAtlas,
    ) {
        let Some(clip) = self.clip else {
            return;
        };
        if !position[0].is_finite()
            || !position[1].is_finite()
            || !scale.is_finite()
            || scale <= 0.0
        {
            return;
        }
        let line_height = atlas.line_height * scale;
        let mut cursor = position;
        for character in text.chars() {
            if character == '\n' {
                cursor = [position[0], cursor[1] + line_height];
                continue;
            }
            let Some(glyph) = atlas.glyph(character) else {
                continue;
            };
            let quad = glyph_quad(cursor, glyph, atlas, scale);
            if let Some(visible) = drawable(quad).and_then(|quad| quad.intersect(clip)) {
                let (uv_min, uv_max) = clipped_uv(glyph, quad, visible);
                self.overlay.push_quad(
                    [visible.x, visible.y, visible.width, visible.height],
                    uv_min,
                    uv_max,
                    color,
                );
            }
            cursor[0] += glyph.advance * scale;
        }
    }

    fn push_clip(&mut self, rect: Rect) {
        let intersection = match (self.clip, drawable(rect)) {
            (Some(current), Some(rect)) => current.intersect(rect),
            _ => None,
        };
        self.clip_stack.push(self.clip);
        self.clip = intersection;
    }

    fn pop_clip(&mut self) {
        if let Some(previous) = self.clip_stack.pop() {
            self.clip = previous;
        }
    }
}

/// A rectangle with finite coordinates and a positive area.
fn drawable(rect: Rect) -> Option<Rect> {
    if rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width > 0.0
        && rect.height > 0.0
    {
        Some(rect)
    } else {
        None
    }
}

/// Pixel-space quad of one glyph at `scale`, with its top-left at `cursor`.
fn glyph_quad(cursor: [f32; 2], glyph: &Glyph, atlas: &FontAtlas, scale: f32) -> Rect {
    Rect::new(
        cursor[0],
        cursor[1],
        (glyph.uv_max[0] - glyph.uv_min[0]) * atlas.width as f32 * scale,
        (glyph.uv_max[1] - glyph.uv_min[1]) * atlas.height as f32 * scale,
    )
}

/// The part of `glyph`'s UV rectangle that corresponds to `visible`.
fn clipped_uv(glyph: &Glyph, quad: Rect, visible: Rect) -> ([f32; 2], [f32; 2]) {
    let u_span = glyph.uv_max[0] - glyph.uv_min[0];
    let v_span = glyph.uv_max[1] - glyph.uv_min[1];
    let u0 = glyph.uv_min[0] + (visible.x - quad.x) / quad.width * u_span;
    let u1 = glyph.uv_min[0] + (visible.right() - quad.x) / quad.width * u_span;
    let v0 = glyph.uv_min[1] + (visible.y - quad.y) / quad.height * v_span;
    let v1 = glyph.uv_min[1] + (visible.bottom() - quad.y) / quad.height * v_span;
    ([u0, v0], [u1, v1])
}

/// Centre of the white texel of `atlas`, used for solid rectangles.
fn white_texel_uv(atlas: &FontAtlas) -> [f32; 2] {
    let [u_min, v_min, u_max, v_max] = atlas.white_uv;
    let u = (u_min + u_max) * 0.5;
    let v = (v_min + v_max) * 0.5;
    if u.is_finite() && v.is_finite() {
        [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]
    } else {
        [0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_ui::{Glyph, TextStyle};

    const COLOR: [f32; 4] = [0.2, 0.4, 0.6, 0.8];

    fn atlas() -> FontAtlas {
        FontAtlas::new(
            16,
            8,
            vec![0; 16 * 8],
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

    fn assert_close(actual: [f32; 2], expected: [f32; 2]) {
        assert!(
            (actual[0] - expected[0]).abs() < 1e-5 && (actual[1] - expected[1]).abs() < 1e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn rectangles_use_the_white_texel_and_the_requested_quad() {
        let mut list = DrawList::new();
        list.rect(Rect::new(10.0, 20.0, 30.0, 40.0), COLOR);
        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 6);
        assert_close(pass.vertices()[0].position, [-0.8, 0.6]);
        assert_close(pass.vertices()[2].position, [-0.2, -0.2]);
        assert_close(pass.vertices()[5].position, [-0.8, -0.2]);
        for vertex in pass.vertices() {
            assert_eq!(vertex.uv, [0.875, 0.75]);
            assert_eq!(vertex.color, COLOR);
        }
    }

    #[test]
    fn text_uses_the_glyph_uv_and_advances_by_its_metric() {
        let mut list = DrawList::new();
        list.text([2.0, 4.0], "AB", TextStyle::new(1.0, COLOR));
        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 12);
        // A: 8×8 quad at (2, 4); B: advance 4, so at (6, 4).
        assert_close(pass.vertices()[1].position, ndc(10.0, 4.0));
        assert_eq!(pass.vertices()[0].uv, [0.0, 0.0]);
        assert_eq!(pass.vertices()[2].uv, [0.5, 1.0]);
        assert_close(pass.vertices()[6].position, ndc(6.0, 4.0));
        assert_eq!(pass.vertices()[6].uv, [0.5, 0.0]);
        assert_eq!(pass.vertices()[8].uv, [1.0, 1.0]);
    }

    #[test]
    fn newline_returns_to_the_run_start_and_moves_one_line() {
        let mut list = DrawList::new();
        list.text([2.0, 4.0], "A\nB", TextStyle::new(1.0, COLOR));
        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_close(pass.vertices()[6].position, ndc(2.0, 14.0));
    }

    #[test]
    fn nested_clips_intersect_and_pop_restores_the_parent() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0));
        list.push_clip(Rect::new(25.0, 25.0, 50.0, 50.0));
        list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);
        list.pop_clip();
        list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);
        list.pop_clip();
        list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);

        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 18);
        assert_close(pass.vertices()[0].position, [-0.5, 0.5]);
        assert_close(pass.vertices()[2].position, [0.0, 0.0]);
        assert_close(pass.vertices()[6].position, [-1.0, 1.0]);
        assert_close(pass.vertices()[8].position, [0.0, 0.0]);
        assert_close(pass.vertices()[12].position, [-1.0, 1.0]);
        assert_close(pass.vertices()[14].position, [1.0, -1.0]);
    }

    #[test]
    fn an_empty_intersection_suppresses_commands_until_the_scope_is_popped() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        list.push_clip(Rect::new(20.0, 20.0, 10.0, 10.0));
        list.rect(Rect::new(0.0, 0.0, 100.0, 100.0), COLOR);
        list.text([0.0, 0.0], "A", TextStyle::new(1.0, COLOR));
        list.pop_clip();
        list.rect(Rect::new(0.0, 0.0, 10.0, 10.0), COLOR);

        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 6);
        assert_close(pass.vertices()[0].position, [-1.0, 1.0]);
        assert_close(pass.vertices()[2].position, [-0.8, 0.8]);
    }

    #[test]
    fn commands_are_emitted_in_list_order() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0));
        list.rect(Rect::new(10.0, 10.0, 10.0, 10.0), COLOR);
        list.text([2.0, 2.0], "A", TextStyle::new(1.0, COLOR));
        list.pop_clip();
        list.rect(Rect::new(60.0, 60.0, 10.0, 10.0), COLOR);

        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 18);
        assert_close(pass.vertices()[0].position, ndc(10.0, 10.0));
        assert_close(pass.vertices()[6].position, ndc(2.0, 2.0));
        assert_eq!(pass.vertices()[6].uv, [0.0, 0.0]);
        assert_close(pass.vertices()[12].position, ndc(60.0, 60.0));
    }

    #[test]
    fn a_partially_clipped_glyph_maps_its_uv_proportionally() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(2.0, 0.0, 4.0, 8.0));
        list.text([0.0, 0.0], "A", TextStyle::new(1.0, COLOR));

        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 6);
        assert_close(pass.vertices()[0].position, ndc(2.0, 0.0));
        assert_close(pass.vertices()[2].position, ndc(6.0, 8.0));
        assert_close(pass.vertices()[0].uv, [0.125, 0.0]);
        assert_close(pass.vertices()[1].uv, [0.375, 0.0]);
        assert_close(pass.vertices()[2].uv, [0.375, 1.0]);
        assert_close(pass.vertices()[5].uv, [0.125, 1.0]);
    }

    #[test]
    fn an_unmatched_pop_keeps_the_current_clip() {
        let mut list = DrawList::new();
        list.pop_clip();
        list.rect(Rect::new(0.0, 0.0, 10.0, 10.0), COLOR);

        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());

        assert_eq!(pass.vertex_count(), 6);
    }

    #[test]
    fn resize_reprojects_and_resets_the_clip() {
        let mut list = DrawList::new();
        list.rect(Rect::new(0.0, 0.0, 50.0, 50.0), COLOR);
        let mut pass = UiPass::new(100, 100);
        pass.rebuild(&list, &atlas());
        assert_close(pass.vertices()[2].position, [0.0, 0.0]);

        pass.resize(200, 200);
        pass.rebuild(&list, &atlas());
        assert_close(pass.vertices()[2].position, [-0.5, 0.5]);
    }

    #[test]
    fn the_fallback_atlas_reproduces_the_overlay_font_metrics() {
        let atlas = fallback_atlas();
        assert_eq!(atlas.width, font::ATLAS_WIDTH);
        assert_eq!(atlas.height, font::ATLAS_HEIGHT);
        assert_eq!(
            atlas.pixels.len(),
            (font::ATLAS_WIDTH * font::ATLAS_HEIGHT) as usize
        );
        assert_eq!(atlas.line_height, (font::CELL_HEIGHT + 2) as f32);

        let upper = atlas.glyph('A').expect("fallback has A");
        let lower = atlas.glyph('a').expect("fallback maps lowercase");
        assert_eq!(upper.uv_min, lower.uv_min);
        assert_eq!(upper.uv_max, lower.uv_max);
        assert_eq!(upper.advance, font::CELL_WIDTH as f32);
        assert_eq!(atlas.glyph(' ').expect("fallback has space").advance, 4.0);

        let (white_min, white_max) = font::cell_uv(font::WHITE_SLOT);
        assert_eq!(
            atlas.white_uv,
            [white_min[0], white_min[1], white_max[0], white_max[1]]
        );
    }

    #[test]
    fn validation_rejects_a_wrong_pixel_count() {
        let mut atlas = atlas();
        atlas.pixels.pop();
        assert_eq!(
            validate_font_atlas(&atlas, 8192),
            Err(FontAtlasError::PixelCount {
                width: 16,
                height: 8,
                expected: 128,
                actual: 127,
            })
        );
    }

    #[test]
    fn validation_rejects_empty_and_out_of_range_atlases() {
        let mut empty = atlas();
        empty.width = 0;
        assert_eq!(
            validate_font_atlas(&empty, 8192),
            Err(FontAtlasError::EmptySize)
        );

        let mut huge = atlas();
        huge.width = 9000;
        huge.pixels = vec![0; 9000 * 8];
        assert_eq!(
            validate_font_atlas(&huge, 8192),
            Err(FontAtlasError::TooLarge {
                width: 9000,
                height: 8,
                max: 8192,
            })
        );

        let mut bad_white = atlas();
        bad_white.white_uv = [0.5, 0.5, 0.4, 1.0];
        assert_eq!(
            validate_font_atlas(&bad_white, 8192),
            Err(FontAtlasError::InvalidWhiteUv([0.5, 0.5, 0.4, 1.0]))
        );
    }

    fn ndc(x: f32, y: f32) -> [f32; 2] {
        [2.0 * x / 100.0 - 1.0, 1.0 - 2.0 * y / 100.0]
    }
}
