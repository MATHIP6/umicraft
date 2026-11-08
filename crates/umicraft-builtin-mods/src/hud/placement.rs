// SPDX-License-Identifier: MIT OR Apache-2.0
//! Where a HUD widget sits on the screen, independent of the resolution.

use serde::{Deserialize, Serialize};

/// Smallest widget scale.
pub const MIN_SCALE: f32 = 0.5;
/// Largest widget scale.
pub const MAX_SCALE: f32 = 3.0;

/// A rectangle in GUI pixels (the scaled coordinates the HUD is drawn in).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HudRect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl HudRect {
    /// Whether the point is inside the rectangle (edges included).
    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

/// One of the nine anchor points of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Top left corner.
    #[default]
    TopLeft,
    /// Middle of the top edge.
    TopCenter,
    /// Top right corner.
    TopRight,
    /// Middle of the left edge.
    MiddleLeft,
    /// Center of the screen.
    Center,
    /// Middle of the right edge.
    MiddleRight,
    /// Bottom left corner.
    BottomLeft,
    /// Middle of the bottom edge.
    BottomCenter,
    /// Bottom right corner.
    BottomRight,
}

impl Anchor {
    /// The anchor as `(horizontal, vertical)` fractions of the free space: 0 is the
    /// left or top, 0.5 the middle, 1 the right or bottom.
    #[must_use]
    pub const fn fractions(self) -> (f32, f32) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::TopCenter => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::MiddleLeft => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::MiddleRight => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::BottomCenter => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }

    /// The anchor whose third of the screen contains the fractions `(fx, fy)` (each 0 to 1).
    #[must_use]
    pub fn nearest(fx: f32, fy: f32) -> Self {
        let third = |f: f32| {
            if f < 1.0 / 3.0 {
                0
            } else if f < 2.0 / 3.0 {
                1
            } else {
                2
            }
        };
        match (third(fx), third(fy)) {
            (0, 0) => Self::TopLeft,
            (1, 0) => Self::TopCenter,
            (2, 0) => Self::TopRight,
            (0, 1) => Self::MiddleLeft,
            (1, 1) => Self::Center,
            (2, 1) => Self::MiddleRight,
            (0, _) => Self::BottomLeft,
            (1, _) => Self::BottomCenter,
            _ => Self::BottomRight,
        }
    }
}

/// Where a widget is, how it is anchored and how big it is drawn.
///
/// The widget is placed at its anchor and then moved by the offset. Because the
/// anchor is relative to the screen, a widget at the bottom right stays at the
/// bottom right when the window is resized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HudPlacement {
    /// Screen point the widget hugs.
    pub anchor: Anchor,
    /// Horizontal shift from the anchored position, in GUI pixels (right is positive).
    pub offset_x: f32,
    /// Vertical shift from the anchored position, in GUI pixels (down is positive).
    pub offset_y: f32,
    /// Size multiplier, within [`MIN_SCALE`] and [`MAX_SCALE`].
    pub scale: f32,
}

impl Default for HudPlacement {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopLeft,
            offset_x: 0.0,
            offset_y: 0.0,
            scale: 1.0,
        }
    }
}

impl HudPlacement {
    /// A widget at `anchor` shifted by `(offset_x, offset_y)`, at normal size.
    #[must_use]
    pub const fn at(anchor: Anchor, offset_x: f32, offset_y: f32) -> Self {
        Self {
            anchor,
            offset_x,
            offset_y,
            scale: 1.0,
        }
    }

    /// Makes the placement safe to use: a non-finite value is replaced and the scale
    /// is clamped. The config file is user-editable.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let scale = if self.scale.is_finite() {
            self.scale.clamp(MIN_SCALE, MAX_SCALE)
        } else {
            1.0
        };
        Self {
            offset_x: finite(self.offset_x),
            offset_y: finite(self.offset_y),
            scale,
            ..self
        }
    }

    /// The on-screen rectangle of a widget whose unscaled size is `size`, on a screen
    /// of `screen` (both in GUI pixels). The rectangle is kept fully on screen
    /// whenever it fits, so a widget can never be lost off-screen.
    #[must_use]
    pub fn rect(&self, size: (f32, f32), screen: (f32, f32)) -> HudRect {
        let (w, h) = (size.0 * self.scale, size.1 * self.scale);
        let (fx, fy) = self.anchor.fractions();
        let x = fx * (screen.0 - w) + self.offset_x;
        let y = fy * (screen.1 - h) + self.offset_y;
        HudRect {
            x: clamp_into(x, w, screen.0),
            y: clamp_into(y, h, screen.1),
            w,
            h,
        }
    }

    /// The placement that puts a widget at `rect` on a screen of `screen`: the
    /// reverse of [`rect`](Self::rect), used when the player drops a dragged widget.
    /// The anchor becomes the nearest of the nine, and the scale is kept.
    #[must_use]
    pub fn from_rect(rect: HudRect, screen: (f32, f32), scale: f32) -> Self {
        let frac = |pos: f32, size: f32, total: f32| {
            if total > 0.0 {
                (pos + size / 2.0) / total
            } else {
                0.0
            }
        };
        let anchor = Anchor::nearest(
            frac(rect.x, rect.w, screen.0),
            frac(rect.y, rect.h, screen.1),
        );
        let (fx, fy) = anchor.fractions();
        Self {
            anchor,
            offset_x: rect.x - fx * (screen.0 - rect.w),
            offset_y: rect.y - fy * (screen.1 - rect.h),
            scale,
        }
        .sanitized()
    }
}

/// Moves `pos` so that `[pos, pos + size]` is inside `[0, total]`, if it fits.
fn clamp_into(pos: f32, size: f32, total: f32) -> f32 {
    if size >= total {
        0.0
    } else {
        pos.clamp(0.0, total - size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f32, f32) = (400.0, 300.0);
    const SIZE: (f32, f32) = (100.0, 20.0);

    #[test]
    fn corners_hug_the_screen_edges() {
        let r = HudPlacement::at(Anchor::BottomRight, 0.0, 0.0).rect(SIZE, SCREEN);
        assert_eq!((r.x, r.y), (300.0, 280.0));
        let r = HudPlacement::at(Anchor::Center, 0.0, 0.0).rect(SIZE, SCREEN);
        assert_eq!((r.x, r.y), (150.0, 140.0));
    }

    #[test]
    fn the_offset_moves_the_widget() {
        let r = HudPlacement::at(Anchor::TopLeft, 10.0, 5.0).rect(SIZE, SCREEN);
        assert_eq!((r.x, r.y), (10.0, 5.0));
    }

    #[test]
    fn the_scale_grows_the_rectangle() {
        let p = HudPlacement {
            scale: 2.0,
            ..HudPlacement::default()
        };
        let r = p.rect(SIZE, SCREEN);
        assert_eq!((r.w, r.h), (200.0, 40.0));
    }

    #[test]
    fn a_widget_cannot_leave_the_screen() {
        let r = HudPlacement::at(Anchor::TopLeft, -500.0, 900.0).rect(SIZE, SCREEN);
        assert_eq!((r.x, r.y), (0.0, 280.0));
    }

    #[test]
    fn a_widget_larger_than_the_screen_sits_at_the_origin() {
        let r = HudPlacement::at(Anchor::Center, 50.0, 50.0).rect((500.0, 400.0), SCREEN);
        assert_eq!((r.x, r.y), (0.0, 0.0));
    }

    #[test]
    fn from_rect_inverts_rect() {
        for anchor in [
            Anchor::TopLeft,
            Anchor::TopRight,
            Anchor::Center,
            Anchor::BottomCenter,
        ] {
            let original = HudPlacement::at(anchor, 0.0, 0.0);
            let rect = original.rect(SIZE, SCREEN);
            let back = HudPlacement::from_rect(rect, SCREEN, 1.0);
            assert_eq!(back.anchor, anchor);
            assert_eq!(back.rect(SIZE, SCREEN), rect);
        }
    }

    #[test]
    fn a_dropped_widget_picks_the_nearest_anchor() {
        let rect = HudRect {
            x: 290.0,
            y: 10.0,
            w: 100.0,
            h: 20.0,
        };
        assert_eq!(
            HudPlacement::from_rect(rect, SCREEN, 1.0).anchor,
            Anchor::TopRight
        );
    }

    #[test]
    fn a_resize_keeps_the_widget_on_its_side() {
        let p = HudPlacement::at(Anchor::BottomRight, -4.0, -4.0);
        let small = p.rect(SIZE, (400.0, 300.0));
        let big = p.rect(SIZE, (800.0, 600.0));
        assert_eq!(small.x + small.w + 4.0, 400.0);
        assert_eq!(big.x + big.w + 4.0, 800.0);
    }

    #[test]
    fn sanitizing_repairs_bad_numbers() {
        let p = HudPlacement {
            anchor: Anchor::Center,
            offset_x: f32::NAN,
            offset_y: f32::INFINITY,
            scale: 99.0,
        }
        .sanitized();
        assert_eq!((p.offset_x, p.offset_y, p.scale), (0.0, 0.0, MAX_SCALE));
        let p = HudPlacement {
            scale: f32::NAN,
            ..HudPlacement::default()
        }
        .sanitized();
        assert!((p.scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn nearest_covers_the_nine_cells() {
        assert_eq!(Anchor::nearest(0.0, 0.0), Anchor::TopLeft);
        assert_eq!(Anchor::nearest(0.5, 0.5), Anchor::Center);
        assert_eq!(Anchor::nearest(1.0, 1.0), Anchor::BottomRight);
        assert_eq!(Anchor::nearest(0.1, 0.9), Anchor::BottomLeft);
    }
}
