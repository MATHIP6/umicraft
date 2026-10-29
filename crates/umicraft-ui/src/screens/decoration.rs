// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sprites cut from a screen's texture: background pieces, flames, arrows.

use crate::geom::Rect;

/// How much of a [`Decoration`] sprite is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    /// The whole sprite.
    Full,
    /// Only the bottom fraction (0..=1) of the sprite, e.g. a burning flame.
    Up(f32),
    /// Only the left fraction (0..=1) of the sprite, e.g. a progress arrow.
    Right(f32),
}

/// A sprite taken from a screen's texture and drawn at a fixed offset.
///
/// All coordinates are unscaled texture pixels; `dst` is relative to the GUI
/// top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decoration {
    /// Top-left of the sprite in the texture.
    pub src: (u32, u32),
    /// Width and height of the whole sprite.
    pub size: (u32, u32),
    /// Where the whole sprite goes, relative to the GUI top-left.
    pub dst: (u32, u32),
    /// Which part of the sprite is drawn.
    pub fill: Fill,
}

/// A texture sub-rectangle and the screen rectangle it is drawn into.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteRect {
    /// `(x, y, width, height)` in texture pixels, ready for
    /// `GuiRegion::sub_region`.
    pub src: (u32, u32, u32, u32),
    /// The scaled destination in physical pixels.
    pub dst: Rect,
}

impl Decoration {
    /// A fully drawn sprite.
    #[must_use]
    pub const fn full(src: (u32, u32), size: (u32, u32), dst: (u32, u32)) -> Self {
        Self {
            src,
            size,
            dst,
            fill: Fill::Full,
        }
    }

    /// The source and destination rectangles to draw, or `None` when the fill
    /// leaves nothing visible.
    ///
    /// `origin` is the GUI top-left in physical pixels and `scale` the GUI
    /// scale. `Up` keeps the bottom rows and `Right` the left columns, so the
    /// visible part stays glued to the same edge as in vanilla.
    #[must_use]
    pub fn sprite_rect(&self, origin: (f32, f32), scale: u32) -> Option<SpriteRect> {
        let (w, h) = self.size;
        let (sx, sy) = self.src;
        let (dx, dy) = self.dst;
        let (width, height, cut_x, cut_y) = match self.fill {
            Fill::Full => (w, h, 0, 0),
            Fill::Up(f) => {
                let kept = visible(h, f);
                (w, kept, 0, h - kept)
            }
            Fill::Right(f) => (visible(w, f), h, 0, 0),
        };
        if width == 0 || height == 0 {
            return None;
        }
        let s = scale as f32;
        Some(SpriteRect {
            src: (sx + cut_x, sy + cut_y, width, height),
            dst: Rect::new(
                origin.0 + (dx + cut_x) as f32 * s,
                origin.1 + (dy + cut_y) as f32 * s,
                width as f32 * s,
                height as f32 * s,
            ),
        })
    }
}

/// The number of pixels of `total` that a fraction keeps.
fn visible(total: u32, fraction: f32) -> u32 {
    let kept = (total as f32 * fraction.clamp(0.0, 1.0)).round();
    (kept as u32).min(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flame(fill: Fill) -> Decoration {
        Decoration {
            src: (176, 0),
            size: (14, 13),
            dst: (56, 36),
            fill,
        }
    }

    #[test]
    fn full_fill_maps_the_whole_sprite_with_scale() {
        let rect = flame(Fill::Full).sprite_rect((10.0, 20.0), 2).unwrap();
        assert_eq!(rect.src, (176, 0, 14, 13));
        assert_eq!(rect.dst, Rect::new(122.0, 92.0, 28.0, 26.0));
    }

    #[test]
    fn up_fill_keeps_the_bottom_rows() {
        // Vanilla: k = 5 draws src y = 12 - k, height k + 1.
        let rect = flame(Fill::Up(6.0 / 13.0))
            .sprite_rect((0.0, 0.0), 1)
            .unwrap();
        assert_eq!(rect.src, (176, 7, 14, 6));
        assert_eq!(rect.dst, Rect::new(56.0, 43.0, 14.0, 6.0));
    }

    #[test]
    fn right_fill_keeps_the_left_columns() {
        let arrow = Decoration {
            src: (176, 14),
            size: (25, 16),
            dst: (79, 34),
            fill: Fill::Right(0.4),
        };
        let rect = arrow.sprite_rect((0.0, 0.0), 1).unwrap();
        assert_eq!(rect.src, (176, 14, 10, 16));
        assert_eq!(rect.dst, Rect::new(79.0, 34.0, 10.0, 16.0));
    }

    #[test]
    fn an_empty_fill_draws_nothing_and_overfull_is_clamped() {
        assert!(flame(Fill::Up(0.0)).sprite_rect((0.0, 0.0), 1).is_none());
        let rect = flame(Fill::Up(3.0)).sprite_rect((0.0, 0.0), 1).unwrap();
        assert_eq!(rect.src, (176, 0, 14, 13));
    }
}
