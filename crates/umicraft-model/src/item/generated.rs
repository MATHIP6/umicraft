// SPDX-License-Identifier: MIT OR Apache-2.0

//! `builtin/generated`: a flat sprite given a one-pixel-thick body.
//!
//! Port of MCP `ItemModelGenerator.makeItemModel`. Each `layerN` texture
//! becomes a front and back plane at z = 7.5..8.5 plus one thin face for
//! every run of pixels that borders a transparent pixel, so a sword or a
//! pickaxe has the extruded silhouette of the vanilla item. The result is a
//! list of ordinary model elements, baked like any other model.
//!
//! The crate reads no textures, so the caller describes each sprite's
//! silhouette with a [`SpriteMask`].

use std::collections::BTreeMap;

use crate::geometry::Face;
use crate::model::document::{Element, FaceSpec};

/// The maximum number of texture layers of a generated model
/// (`ItemModelGenerator.LAYERS`).
pub const MAX_LAYERS: usize = 5;

/// Half the model thickness, in sixteenths: the body spans 7.5..8.5.
const BODY_FRONT: f64 = 7.5;
const BODY_BACK: f64 = 8.5;

/// Which pixels of a sprite are visible (alpha above zero).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteMask {
    width: u32,
    height: u32,
    solid: Vec<bool>,
}

impl SpriteMask {
    /// A fully opaque sprite, used when a texture is unknown (the vanilla
    /// missing texture has no transparent pixel).
    #[must_use]
    pub fn opaque(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            solid: vec![true; width as usize * height as usize],
        }
    }

    /// Builds a mask from RGBA8 pixels, the first frame of the sprite.
    ///
    /// Returns `None` when `rgba` does not hold `width * height` pixels or
    /// the sprite is empty.
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        let count = (width as usize).checked_mul(height as usize)?;
        if width < 2 || height < 2 || rgba.len() != count.checked_mul(4)? {
            return None;
        }
        let solid = rgba.chunks_exact(4).map(|pixel| pixel[3] != 0).collect();
        Some(Self {
            width,
            height,
            solid,
        })
    }

    /// Whether the pixel is transparent; outside the sprite counts as
    /// transparent (`ItemModelGenerator.func_178391_a`).
    fn is_clear(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return true;
        }
        !self.solid[y as usize * self.width as usize + x as usize]
    }
}

/// Builds the elements of a generated model, one group per layer.
///
/// `layers[i]` is the mask of `layer{i}`; the faces reference `#layer{i}` and
/// carry the layer index as tint index.
#[must_use]
pub(crate) fn generate_elements(layers: &[SpriteMask]) -> Vec<Element> {
    layers
        .iter()
        .take(MAX_LAYERS)
        .enumerate()
        .flat_map(|(index, mask)| layer_elements(index, mask))
        .collect()
}

fn layer_elements(layer: usize, mask: &SpriteMask) -> Vec<Element> {
    let texture = format!("#layer{layer}");
    let mut elements = vec![front_and_back(layer, &texture)];
    elements.extend(
        spans(mask)
            .iter()
            .map(|span| side_element(span, mask, layer, &texture)),
    );
    elements
}

fn face(texture: &str, layer: usize, uv: [f64; 4]) -> FaceSpec {
    FaceSpec {
        texture: texture.to_string(),
        uv: Some(uv),
        cullface: None,
        rotation: 0,
        tint: Some(layer as u32),
    }
}

fn front_and_back(layer: usize, texture: &str) -> Element {
    Element {
        from: [0.0, 0.0, BODY_FRONT],
        to: [16.0, 16.0, BODY_BACK],
        rotation: None,
        faces: BTreeMap::from([
            (Face::South, face(texture, layer, [0.0, 0.0, 16.0, 16.0])),
            (Face::North, face(texture, layer, [16.0, 0.0, 0.0, 16.0])),
        ]),
        shade: true,
    }
}

/// The side a span faces (`ItemModelGenerator.SpanFacing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Up,
    Down,
    Left,
    Right,
}

impl Side {
    const ALL: [Self; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    /// The neighbouring pixel offset that must be clear for this side.
    const fn offset(self) -> (i64, i64) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }

    /// The model face drawn for this side.
    const fn face(self) -> Face {
        match self {
            Self::Up => Face::Up,
            Self::Down => Face::Down,
            Self::Left => Face::East,
            Self::Right => Face::West,
        }
    }

    /// Whether the span runs along a pixel row (up and down sides).
    const fn along_row(self) -> bool {
        matches!(self, Self::Up | Self::Down)
    }
}

/// A run of adjacent pixels sharing one exposed side.
#[derive(Debug, Clone, Copy)]
struct Span {
    side: Side,
    min: i64,
    max: i64,
    /// The row (up/down) or column (left/right) of the run.
    line: i64,
}

/// Finds the exposed runs of a sprite, merging by side and line exactly like
/// the reference (a gap in a line is not split).
fn spans(mask: &SpriteMask) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for y in 0..i64::from(mask.height) {
        for x in 0..i64::from(mask.width) {
            if mask.is_clear(x, y) {
                continue;
            }
            for side in Side::ALL {
                let (dx, dy) = side.offset();
                if mask.is_clear(x + dx, y + dy) {
                    add_pixel(&mut spans, side, x, y);
                }
            }
        }
    }
    spans
}

fn add_pixel(spans: &mut Vec<Span>, side: Side, x: i64, y: i64) {
    let (line, along) = if side.along_row() { (y, x) } else { (x, y) };
    match spans
        .iter_mut()
        .find(|span| span.side == side && span.line == line)
    {
        Some(span) => {
            span.min = span.min.min(along);
            span.max = span.max.max(along);
        }
        None => spans.push(Span {
            side,
            min: along,
            max: along,
            line,
        }),
    }
}

/// The thin element of one span (`ItemModelGenerator.func_178397_a`).
///
/// Positions are in pixels of the sprite, converted to sixteenths of a block
/// (y flipped); the texture rectangle is the same pixels scaled so that the
/// face samples the one-pixel strip it covers.
fn side_element(span: &Span, mask: &SpriteMask, layer: usize, texture: &str) -> Element {
    let (w, h) = (mask.width as f32, mask.height as f32);
    let (start, end, line) = (span.min as f32, span.max as f32 + 1.0, span.line as f32);
    // Pixel extent of the face along x and y, and of its texture strip.
    let (x, y, u, v, scale_u, scale_v) = match span.side {
        Side::Up => (
            (start, end),
            (line, line),
            (start, end),
            (line, line),
            16.0 / w,
            16.0 / (h - 1.0),
        ),
        Side::Down => (
            (start, end),
            (line + 1.0, line + 1.0),
            (start, end),
            (line, line),
            16.0 / w,
            16.0 / (h - 1.0),
        ),
        Side::Left => (
            (line, line),
            (start, end),
            (line, line),
            (start, end),
            16.0 / (w - 1.0),
            16.0 / h,
        ),
        Side::Right => (
            (line + 1.0, line + 1.0),
            (start, end),
            (line, line),
            (start, end),
            16.0 / (w - 1.0),
            16.0 / h,
        ),
    };
    let to_x = |pixel: f32| f64::from(pixel * (16.0 / w));
    let to_y = |pixel: f32| f64::from(16.0 - pixel * (16.0 / h));
    let uv = [
        f64::from(u.0 * scale_u),
        f64::from(v.0 * scale_v),
        f64::from(u.1 * scale_u),
        f64::from(v.1 * scale_v),
    ];
    // Up and down sides span x at one y; left and right span y at one x.
    let (from_x, from_y) = (to_x(x.0), to_y(y.0));
    let (to_corner_x, to_corner_y) = if span.side.along_row() {
        (to_x(x.1), from_y)
    } else {
        (from_x, to_y(y.1))
    };
    Element {
        from: [from_x, from_y, BODY_FRONT],
        to: [to_corner_x, to_corner_y, BODY_BACK],
        rotation: None,
        faces: BTreeMap::from([(span.side.face(), face(texture, layer, uv))]),
        shade: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16x16 sprite with only the pixel `(x, y)` visible.
    fn single_pixel(x: usize, y: usize) -> SpriteMask {
        let mut rgba = vec![0_u8; 16 * 16 * 4];
        rgba[(y * 16 + x) * 4 + 3] = 255;
        SpriteMask::from_rgba(16, 16, &rgba).expect("16x16")
    }

    #[test]
    fn a_lone_pixel_gets_a_body_and_four_sides() {
        let elements = generate_elements(&[single_pixel(3, 4)]);
        assert_eq!(elements.len(), 5);
        assert_eq!(elements[0].faces.len(), 2);
        assert!(elements[1..].iter().all(|element| element.faces.len() == 1));
    }

    #[test]
    fn the_up_side_sits_on_the_top_edge_of_the_pixel() {
        let elements = generate_elements(&[single_pixel(3, 4)]);
        let up = elements
            .iter()
            .find(|element| element.faces.contains_key(&Face::Up))
            .expect("up side");
        assert_eq!(up.from, [3.0, 12.0, 7.5]);
        assert_eq!(up.to, [4.0, 12.0, 8.5]);
        let spec = &up.faces[&Face::Up];
        assert_eq!(spec.tint, Some(0));
        assert_eq!(spec.texture, "#layer0");
    }

    #[test]
    fn a_run_of_pixels_is_one_span_per_side() {
        let mut rgba = vec![0_u8; 16 * 16 * 4];
        for x in 2..6 {
            rgba[(5 * 16 + x) * 4 + 3] = 255;
        }
        let mask = SpriteMask::from_rgba(16, 16, &rgba).expect("16x16");
        // body + up + down (one each for the 4-wide run) + left + right
        assert_eq!(generate_elements(&[mask]).len(), 5);
    }

    #[test]
    fn layers_are_tinted_by_their_index_and_capped() {
        let layers = vec![SpriteMask::opaque(16, 16); 7];
        let elements = generate_elements(&layers);
        let last = elements.last().expect("elements");
        assert_eq!(last.faces.values().next().and_then(|f| f.tint), Some(4));
    }

    #[test]
    fn rejects_malformed_pixel_data() {
        assert!(SpriteMask::from_rgba(16, 16, &[0; 10]).is_none());
        assert!(SpriteMask::from_rgba(1, 1, &[0; 4]).is_none());
    }
}
