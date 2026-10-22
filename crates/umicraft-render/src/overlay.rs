// SPDX-License-Identifier: MIT OR Apache-2.0

//! Immediate-mode debug overlay: text, rectangles, and a frame-time graph.
//!
//! Vertices are already in NDC, so the overlay shader is a pass-through. All
//! drawing is rebuilt every frame; the renderer uploads the vertex list.

use bytemuck::{Pod, Zeroable};

use crate::font;

/// One overlay vertex in NDC with a font-atlas UV and RGBA colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct OverlayVertex {
    /// Position in clip space (−1..1, Y up).
    pub position: [f32; 2],
    /// Font-atlas UV.
    pub uv: [f32; 2],
    /// Straight-alpha RGBA colour.
    pub color: [f32; 4],
}

/// Screen-space overlay vertex builder for one frame.
#[derive(Debug, Clone)]
pub struct Overlay {
    width: f32,
    height: f32,
    vertices: Vec<OverlayVertex>,
}

impl Overlay {
    /// Creates an overlay for a viewport of `width`×`height` physical pixels.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1) as f32,
            height: height.max(1) as f32,
            vertices: Vec::new(),
        }
    }

    /// Updates the viewport size (window resize).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as f32;
        self.height = height.max(1) as f32;
    }

    /// Drops all geometry for the next frame.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// True when nothing would be drawn.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Number of vertices to draw.
    #[must_use]
    pub fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }

    /// Borrows the vertex list for upload.
    #[must_use]
    pub fn vertices(&self) -> &[OverlayVertex] {
        &self.vertices
    }

    /// Draws a solid rectangle. Coordinates are top-left origin pixels.
    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) {
        let uv = font::white_uv();
        self.push_quad([x, y, width, height], uv, uv, color);
    }

    /// Draws uppercase text. Lowercase maps to uppercase; unknown characters
    /// render as spaces. `scale` multiplies the 3×5 glyph.
    pub fn text(&mut self, x: f32, y: f32, scale: f32, color: [f32; 4], text: &str) {
        let advance = font::CELL_WIDTH as f32 * scale;
        let line_height = (font::CELL_HEIGHT as f32 + 2.0) * scale;
        let mut cursor_x = x;
        let mut cursor_y = y;
        for character in text.chars() {
            if character == '\n' {
                cursor_x = x;
                cursor_y += line_height;
                continue;
            }
            if character != ' ' {
                let (uv_min, uv_max) = font::cell_uv(font::slot(character));
                self.push_quad(
                    [
                        cursor_x,
                        cursor_y,
                        font::CELL_WIDTH as f32 * scale,
                        font::CELL_HEIGHT as f32 * scale,
                    ],
                    uv_min,
                    uv_max,
                    color,
                );
            }
            cursor_x += advance;
        }
    }

    /// Width in pixels that [`Overlay::text`] would occupy (single line).
    #[must_use]
    pub fn text_width(text: &str, scale: f32) -> f32 {
        text.chars().count() as f32 * font::CELL_WIDTH as f32 * scale
    }

    /// Draws a frame-time history graph, newest sample on the right.
    ///
    /// Green is ≤16.7 ms, yellow ≤33.3 ms, red slower; the bar height is
    /// clamped at 50 ms.
    pub fn frame_graph(&mut self, x: f32, y: f32, width: f32, height: f32, samples: &[f32]) {
        self.rect(x, y, width, height, [0.0, 0.0, 0.0, 0.55]);
        if samples.is_empty() || width <= 0.0 || height <= 0.0 {
            return;
        }
        let bar_width = (width / samples.len() as f32).max(1.0);
        for (index, millis) in samples.iter().enumerate() {
            let fraction = (millis / 50.0).clamp(0.0, 1.0);
            let bar_height = (height * fraction).max(1.0);
            let color = if *millis <= 16.7 {
                [0.35, 0.9, 0.35, 0.9]
            } else if *millis <= 33.3 {
                [0.95, 0.8, 0.25, 0.9]
            } else {
                [0.95, 0.3, 0.25, 0.9]
            };
            self.rect(
                x + index as f32 * bar_width,
                y + height - bar_height,
                bar_width,
                bar_height,
                color,
            );
        }
    }

    /// Appends one pixel-space quad projected to NDC.
    ///
    /// Shared with [`crate::ui_pass`], which clips quads before calling it.
    pub(crate) fn push_quad(
        &mut self,
        [x, y, width, height]: [f32; 4],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
    ) {
        let x0 = 2.0 * x / self.width - 1.0;
        let x1 = 2.0 * (x + width) / self.width - 1.0;
        let y0 = 1.0 - 2.0 * y / self.height;
        let y1 = 1.0 - 2.0 * (y + height) / self.height;
        let vertex = |position: [f32; 2], uv: [f32; 2]| OverlayVertex {
            position,
            uv,
            color,
        };
        self.vertices.extend_from_slice(&[
            vertex([x0, y0], uv_min),
            vertex([x1, y0], [uv_max[0], uv_min[1]]),
            vertex([x1, y1], uv_max),
            vertex([x0, y0], uv_min),
            vertex([x1, y1], uv_max),
            vertex([x0, y1], [uv_min[0], uv_max[1]]),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_emits_one_quad_per_non_space_glyph() {
        let mut overlay = Overlay::new(1280, 720);
        overlay.text(10.0, 10.0, 2.0, [1.0; 4], "FPS 60");
        assert_eq!(overlay.vertex_count(), 5 * 6);
        assert!(!overlay.is_empty());
        overlay.clear();
        assert!(overlay.is_empty());
    }

    #[test]
    fn vertices_stay_inside_ndc_for_on_screen_text() {
        let mut overlay = Overlay::new(1280, 720);
        overlay.text(10.0, 10.0, 2.0, [1.0; 4], "XYZ -1698 4 111");
        for vertex in overlay.vertices() {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }

    #[test]
    fn rectangles_and_graphs_are_triangles() {
        let mut overlay = Overlay::new(800, 600);
        overlay.rect(0.0, 0.0, 10.0, 10.0, [1.0; 4]);
        assert_eq!(overlay.vertex_count(), 6);
        overlay.frame_graph(0.0, 0.0, 100.0, 40.0, &[8.0, 20.0, 40.0]);
        assert_eq!(overlay.vertex_count(), 6 * (1 + 1 + 3));
    }

    #[test]
    fn resize_updates_projection() {
        let mut overlay = Overlay::new(100, 100);
        overlay.rect(0.0, 0.0, 50.0, 50.0, [1.0; 4]);
        assert_eq!(overlay.vertices()[0].position, [-1.0, 1.0]);
        overlay.resize(200, 200);
        overlay.clear();
        overlay.rect(0.0, 0.0, 50.0, 50.0, [1.0; 4]);
        assert_eq!(overlay.vertices()[0].position, [-1.0, 1.0]);
        assert_eq!(overlay.vertices()[2].position, [-0.5, 0.5]);
    }
}
