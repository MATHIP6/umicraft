// SPDX-License-Identifier: MIT OR Apache-2.0

//! Screen-space geometry in physical pixels, with a top-left origin.
//!
//! The UI layer uses the same pixel space as the renderer's overlay
//! (`Overlay::rect` and `Overlay::text` in `umicraft-render`), so a widget can
//! be drawn without a coordinate conversion. [`Rect`] is the only geometry
//! type in the frozen M4 contract; layout helpers build on it.

/// A rectangle in physical pixels, top-left origin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width in pixels; never negative for a rect produced by the layout.
    pub width: f32,
    /// Height in pixels; never negative for a rect produced by the layout.
    pub height: f32,
}

impl Rect {
    /// The empty rectangle at the origin.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    /// Creates a rectangle from its top-left corner and its size.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The right edge (`x + width`).
    #[must_use]
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge (`y + height`).
    #[must_use]
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// True when the rectangle has no area.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// True when `point` is inside the rectangle, edges included.
    #[must_use]
    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] <= self.right()
            && point[1] >= self.y
            && point[1] <= self.bottom()
    }

    /// The intersection of two rectangles, or `None` when they do not overlap.
    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            None
        } else {
            Some(Self::new(x, y, right - x, bottom - y))
        }
    }

    /// Shrinks the rectangle by `amount` on every side.
    ///
    /// The result is clamped to an empty rectangle when the inset is larger
    /// than half of a dimension; callers never have to handle negative sizes.
    #[must_use]
    pub fn inset(self, amount: f32) -> Self {
        let double = amount * 2.0;
        if double >= self.width || double >= self.height {
            return Self::new(self.x, self.y, 0.0, 0.0);
        }
        Self::new(
            self.x + amount,
            self.y + amount,
            self.width - double,
            self.height - double,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn edges_and_containment() {
        let rect = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(rect.right(), 40.0);
        assert_eq!(rect.bottom(), 60.0);
        assert!(rect.contains([10.0, 20.0]));
        assert!(rect.contains([40.0, 60.0]));
        assert!(!rect.contains([9.9, 20.0]));
        assert!(!rect.contains([10.0, 60.1]));
    }

    #[test]
    fn intersection_handles_overlap_and_gap() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersect(b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        let gap = Rect::new(20.0, 20.0, 5.0, 5.0);
        assert_eq!(a.intersect(gap), None);
    }

    #[test]
    fn inset_clamps_to_empty() {
        let rect = Rect::new(10.0, 10.0, 20.0, 10.0);
        assert_eq!(rect.inset(2.0), Rect::new(12.0, 12.0, 16.0, 6.0));
        assert!(rect.inset(10.0).is_empty());
    }
}
