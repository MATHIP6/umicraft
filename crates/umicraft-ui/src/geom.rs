// SPDX-License-Identifier: MIT OR Apache-2.0

//! Screen-space geometry in physical pixels, with a top-left origin.
//!
//! The UI layer uses the same pixel space as the renderer's overlay
//! (`Overlay::rect` and `Overlay::text` in `umicraft-render`), so a widget can
//! be drawn without a coordinate conversion. [`Rect`] is the only geometry
//! type in the frozen M4 contract; [`Insets`], [`Layout`] and [`ScrollArea`]
//! build the pixel-exact stacks and scroll viewports on top of it.
//!
//! Layout intentionally keeps no child list: an immediate-mode widget asks the
//! layout for the next rectangle and draws immediately. That keeps the frame
//! allocation-light and makes every rule in this module a pure function that
//! can be tested without a GPU or a window.

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
    ///
    /// An empty rectangle never contains a point, even when the point equals
    /// its origin; this keeps an empty clip region from letting a click
    /// through.
    #[must_use]
    pub fn contains(self, point: [f32; 2]) -> bool {
        !self.is_empty()
            && point[0] >= self.x
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
        self.deflate(Insets::all(amount))
    }

    /// Shrinks the rectangle by different amounts on each side.
    ///
    /// The corner stays at the original top-left plus the left/top insets and
    /// the size never becomes negative. A rectangle that is too small for the
    /// insets collapses to its (shifted) origin with a zero size.
    #[must_use]
    pub fn deflate(self, insets: Insets) -> Self {
        let width = (self.width - insets.left - insets.right).max(0.0);
        let height = (self.height - insets.top - insets.bottom).max(0.0);
        Self::new(self.x + insets.left, self.y + insets.top, width, height)
    }

    /// Moves the rectangle by `(dx, dy)` without changing its size.
    #[must_use]
    pub fn translate(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}

/// The four insets around a content area, in physical pixels.
///
/// Unlike [`Rect::inset`], which applies one amount to every side, this type
/// describes asymmetric padding (for example a panel with a title strip on
/// top). All fields are non-negative for values produced by [`Insets::new`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Insets {
    /// Inset from the left edge.
    pub left: f32,
    /// Inset from the top edge.
    pub top: f32,
    /// Inset from the right edge.
    pub right: f32,
    /// Inset from the bottom edge.
    pub bottom: f32,
}

impl Insets {
    /// No inset on any side.
    pub const ZERO: Self = Self::all(0.0);

    /// The same inset on every side.
    #[must_use]
    pub const fn all(amount: f32) -> Self {
        Self {
            left: amount,
            top: amount,
            right: amount,
            bottom: amount,
        }
    }

    /// One inset for the horizontal sides, another for the vertical ones.
    #[must_use]
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }

    /// Insets in clockwise reading order: left, top, right, bottom.
    #[must_use]
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The total horizontal inset (`left + right`).
    #[must_use]
    pub fn horizontal_sum(self) -> f32 {
        self.left + self.right
    }

    /// The total vertical inset (`top + bottom`).
    #[must_use]
    pub fn vertical_sum(self) -> f32 {
        self.top + self.bottom
    }
}

/// The direction in which a [`Layout`] stacks its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Children advance left to right.
    Horizontal,
    /// Children advance top to bottom.
    Vertical,
}

/// A cursor that carves child rectangles out of one content area.
///
/// `Layout` is a plain value, so a widget can copy it, lay out a subtree, and
/// leave the caller's cursor untouched. The stack starts at the top-left
/// corner of [`Layout::area`], advances by each item's main-axis size and adds
/// `spacing` only *between* two non-empty items. Empty items neither advance
/// the cursor nor create a gap.
///
/// # Example
///
/// ```
/// use umicraft_ui::geom::{Layout, Rect};
///
/// let mut layout = Layout::vertical(Rect::new(10.0, 20.0, 100.0, 80.0)).with_spacing(4.0);
/// assert_eq!(layout.next(60.0, 10.0), Rect::new(10.0, 20.0, 60.0, 10.0));
/// assert_eq!(layout.next(60.0, 20.0), Rect::new(10.0, 34.0, 60.0, 20.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    area: Rect,
    axis: Axis,
    cursor: f32,
    spacing: f32,
    placed: bool,
}

impl Layout {
    /// Creates a stack inside `area` that advances along `axis`.
    #[must_use]
    pub fn new(area: Rect, axis: Axis) -> Self {
        let cursor = match axis {
            Axis::Horizontal => area.x,
            Axis::Vertical => area.y,
        };
        Self {
            area,
            axis,
            cursor,
            spacing: 0.0,
            placed: false,
        }
    }

    /// A top-to-bottom stack inside `area`.
    #[must_use]
    pub fn vertical(area: Rect) -> Self {
        Self::new(area, Axis::Vertical)
    }

    /// A left-to-right stack inside `area`.
    #[must_use]
    pub fn horizontal(area: Rect) -> Self {
        Self::new(area, Axis::Horizontal)
    }

    /// Sets the pixel gap inserted between two consecutive non-empty items.
    #[must_use]
    pub fn with_spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    /// The content area the stack was created in.
    #[must_use]
    pub const fn area(self) -> Rect {
        self.area
    }

    /// The stacking direction.
    #[must_use]
    pub const fn axis(self) -> Axis {
        self.axis
    }

    /// The gap inserted between two items.
    #[must_use]
    pub const fn spacing(self) -> f32 {
        self.spacing
    }

    /// The part of the area that has not been consumed yet.
    ///
    /// The returned rectangle starts at the next item position (without the
    /// inter-item gap) and is empty once the cursor reaches the end.
    #[must_use]
    pub fn remaining(self) -> Rect {
        match self.axis {
            Axis::Horizontal => Rect::new(
                self.cursor,
                self.area.y,
                (self.area.right() - self.cursor).max(0.0),
                self.area.height,
            ),
            Axis::Vertical => Rect::new(
                self.area.x,
                self.cursor,
                self.area.width,
                (self.area.bottom() - self.cursor).max(0.0),
            ),
        }
    }

    /// Places an item of the requested size at the cursor and advances it.
    ///
    /// The cross-axis size is clamped to the content area, so a row can never
    /// be wider than the panel that contains it. The main-axis size is kept as
    /// requested (a scroll list is allowed to lay out rows past the visible
    /// area). An item with a non-positive main-axis size is returned as an
    /// empty rectangle at the cursor and does not advance the stack.
    #[must_use]
    pub fn next(&mut self, width: f32, height: f32) -> Rect {
        let (main, cross) = match self.axis {
            Axis::Horizontal => (width, height.clamp(0.0, self.area.height.max(0.0))),
            Axis::Vertical => (height, width.clamp(0.0, self.area.width.max(0.0))),
        };
        if main <= 0.0 {
            return match self.axis {
                Axis::Horizontal => Rect::new(self.cursor, self.area.y, 0.0, cross),
                Axis::Vertical => Rect::new(self.area.x, self.cursor, cross, 0.0),
            };
        }
        if self.placed {
            self.cursor += self.spacing;
        }
        let rect = match self.axis {
            Axis::Horizontal => Rect::new(self.cursor, self.area.y, main, cross),
            Axis::Vertical => Rect::new(self.area.x, self.cursor, cross, main),
        };
        self.cursor += main;
        self.placed = true;
        rect
    }

    /// Places an item that fills the cross axis and has the given main size.
    #[must_use]
    pub fn next_fixed(&mut self, main: f32) -> Rect {
        match self.axis {
            Axis::Horizontal => self.next(main, self.area.height),
            Axis::Vertical => self.next(self.area.width, main),
        }
    }

    /// Places one item that consumes the remaining main axis.
    ///
    /// The inter-item gap is subtracted from the available space when at least
    /// one item was already placed, so the returned rectangle ends exactly at
    /// the content edge. When no space remains, an empty rectangle at the
    /// cursor is returned.
    #[must_use]
    pub fn next_remaining(&mut self) -> Rect {
        let available = match self.axis {
            Axis::Horizontal => (self.area.right() - self.cursor).max(0.0),
            Axis::Vertical => (self.area.bottom() - self.cursor).max(0.0),
        };
        let gap = if self.placed { self.spacing } else { 0.0 };
        let main = (available - gap).max(0.0);
        match self.axis {
            Axis::Horizontal => self.next(main, self.area.height),
            Axis::Vertical => self.next(self.area.width, main),
        }
    }
}

/// A vertical scroll viewport over content taller than the viewport.
///
/// The area owns only the pure scrolling maths: the clamped offset, the
/// content rectangle, and the intersection used to clip rows. The wheel and
/// keyboard handling belong to the list widget. Content coordinates start at
/// zero at the top of the content; a row at content `y` is visible in the
/// viewport when it overlaps [`ScrollArea::viewport`].
///
/// # Example
///
/// ```
/// use umicraft_ui::geom::{Rect, ScrollArea};
///
/// let mut scroll = ScrollArea::new(Rect::new(0.0, 10.0, 200.0, 100.0));
/// scroll.set_content_height(250.0);
/// scroll.set_offset(500.0);
/// assert_eq!(scroll.offset(), 150.0);
/// assert_eq!(scroll.content_rect().y, -140.0);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollArea {
    viewport: Rect,
    content_height: f32,
    offset: f32,
}

impl ScrollArea {
    /// Creates a viewport at the top of the content.
    #[must_use]
    pub fn new(viewport: Rect) -> Self {
        Self {
            viewport,
            content_height: 0.0,
            offset: 0.0,
        }
    }

    /// The visible rectangle in screen space.
    #[must_use]
    pub const fn viewport(self) -> Rect {
        self.viewport
    }

    /// Replaces the visible rectangle and clamps the offset to the new height.
    pub fn set_viewport(&mut self, viewport: Rect) {
        self.viewport = viewport;
        self.clamp_offset();
    }

    /// The total height of the scrolled content.
    #[must_use]
    pub const fn content_height(self) -> f32 {
        self.content_height
    }

    /// Replaces the content height and clamps the offset to the new range.
    pub fn set_content_height(&mut self, content_height: f32) {
        self.content_height = content_height.max(0.0);
        self.clamp_offset();
    }

    /// The current scroll offset; zero is the top of the content.
    #[must_use]
    pub const fn offset(self) -> f32 {
        self.offset
    }

    /// Scrolls to `offset`, clamped to `0..=max_offset`.
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = if offset.is_nan() { 0.0 } else { offset };
        self.clamp_offset();
    }

    /// The largest offset that still shows the bottom of the content.
    #[must_use]
    pub fn max_offset(self) -> f32 {
        (self.content_height - self.viewport.height).max(0.0)
    }

    /// Adds `delta` pixels to the offset; positive values scroll down.
    pub fn scroll_by(&mut self, delta: f32) {
        self.set_offset(self.offset + delta);
    }

    /// The whole content rectangle in screen space.
    ///
    /// Its height is [`ScrollArea::content_height`] and its top is the
    /// viewport top minus the current offset. It can extend above or below the
    /// viewport; callers intersect individual rows before drawing them.
    #[must_use]
    pub fn content_rect(self) -> Rect {
        Rect::new(
            self.viewport.x,
            self.viewport.y - self.offset,
            self.viewport.width,
            self.content_height,
        )
    }

    /// A vertical layout over the whole content rectangle.
    ///
    /// Rows laid out past the viewport are still assigned rectangles; use
    /// [`ScrollArea::visible_rect`] or [`ScrollArea::is_visible`] to skip the
    /// ones that the clip would discard.
    #[must_use]
    pub fn content_layout(self) -> Layout {
        Layout::vertical(self.content_rect())
    }

    /// The visible part of a **screen-space** `rect`, or `None` when it lies
    /// outside the viewport.
    ///
    /// Rows produced by [`ScrollArea::content_layout`] are already in screen
    /// space, so they can be passed directly.
    #[must_use]
    pub fn visible_rect(self, rect: Rect) -> Option<Rect> {
        self.viewport.intersect(rect)
    }

    /// True when `rect` overlaps the visible viewport.
    #[must_use]
    pub fn is_visible(self, rect: Rect) -> bool {
        self.visible_rect(rect).is_some()
    }

    /// Clamps the offset after the content or the viewport changed.
    fn clamp_offset(&mut self) {
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }
}

#[cfg(test)]
mod tests {
    use super::{Axis, Insets, Layout, Rect, ScrollArea};

    #[test]
    fn edges_and_containment() {
        let rect = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(rect.right(), 40.0);
        assert_eq!(rect.bottom(), 60.0);
        assert!(rect.contains([10.0, 20.0]));
        assert!(rect.contains([40.0, 60.0]));
        assert!(!rect.contains([9.9, 20.0]));
        assert!(!rect.contains([10.0, 60.1]));
        assert!(!Rect::ZERO.contains([0.0, 0.0]));
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

    #[test]
    fn deflate_applies_asymmetric_insets() {
        let rect = Rect::new(10.0, 20.0, 100.0, 60.0);
        let insets = Insets::new(4.0, 8.0, 10.0, 12.0);
        assert_eq!(rect.deflate(insets), Rect::new(14.0, 28.0, 86.0, 40.0));
        assert_eq!(insets.horizontal_sum(), 14.0);
        assert_eq!(insets.vertical_sum(), 20.0);
        assert_eq!(Insets::symmetric(3.0, 7.0), Insets::new(3.0, 7.0, 3.0, 7.0));
    }

    #[test]
    fn vertical_layout_places_rows_with_exact_pixels() {
        let area = Rect::new(10.0, 20.0, 100.0, 80.0);
        let mut layout = Layout::vertical(area).with_spacing(4.0);
        assert_eq!(layout.axis(), Axis::Vertical);
        assert_eq!(layout.area(), area);

        assert_eq!(layout.next(60.0, 10.0), Rect::new(10.0, 20.0, 60.0, 10.0));
        assert_eq!(layout.next(120.0, 20.0), Rect::new(10.0, 34.0, 100.0, 20.0));
        assert_eq!(layout.next(0.0, 0.0), Rect::new(10.0, 54.0, 0.0, 0.0));
        assert_eq!(layout.next_fixed(12.0), Rect::new(10.0, 58.0, 100.0, 12.0));
        assert_eq!(layout.remaining(), Rect::new(10.0, 70.0, 100.0, 30.0));
        assert_eq!(layout.next_remaining(), Rect::new(10.0, 74.0, 100.0, 26.0));
        assert!(layout.remaining().is_empty());
    }

    #[test]
    fn horizontal_layout_places_columns_with_exact_pixels() {
        let area = Rect::new(0.0, 0.0, 120.0, 40.0);
        let mut layout = Layout::horizontal(area).with_spacing(5.0);
        assert_eq!(layout.next(20.0, 999.0), Rect::new(0.0, 0.0, 20.0, 40.0));
        assert_eq!(layout.next(30.0, 10.0), Rect::new(25.0, 0.0, 30.0, 10.0));
        assert_eq!(layout.remaining(), Rect::new(55.0, 0.0, 65.0, 40.0));
        assert_eq!(layout.next_remaining(), Rect::new(60.0, 0.0, 60.0, 40.0));
    }

    #[test]
    fn empty_items_do_not_create_gaps() {
        let mut layout = Layout::vertical(Rect::new(0.0, 0.0, 10.0, 10.0)).with_spacing(3.0);
        assert_eq!(layout.next(10.0, 0.0), Rect::new(0.0, 0.0, 10.0, 0.0));
        assert_eq!(layout.next(10.0, 4.0), Rect::new(0.0, 0.0, 10.0, 4.0));
        assert_eq!(layout.next(0.0, 0.0), Rect::new(0.0, 4.0, 0.0, 0.0));
        assert_eq!(layout.next(10.0, 4.0), Rect::new(0.0, 7.0, 10.0, 4.0));
    }

    #[test]
    fn a_layout_can_be_copied_without_moving_the_original() {
        let mut outer = Layout::vertical(Rect::new(0.0, 0.0, 100.0, 100.0));
        let _first = outer.next_fixed(10.0);
        let mut inner = outer;
        assert_eq!(inner.next_fixed(20.0), Rect::new(0.0, 10.0, 100.0, 20.0));
        assert_eq!(outer.next_fixed(5.0), Rect::new(0.0, 10.0, 100.0, 5.0));
    }

    #[test]
    fn scroll_area_clamps_and_offsets_content() {
        let mut scroll = ScrollArea::new(Rect::new(0.0, 10.0, 200.0, 100.0));
        scroll.set_content_height(250.0);
        assert_eq!(scroll.max_offset(), 150.0);
        scroll.set_offset(500.0);
        assert_eq!(scroll.offset(), 150.0);
        assert_eq!(scroll.content_rect(), Rect::new(0.0, -140.0, 200.0, 250.0));

        scroll.set_offset(-20.0);
        assert_eq!(scroll.offset(), 0.0);
        scroll.scroll_by(25.0);
        assert_eq!(scroll.offset(), 25.0);
        assert_eq!(scroll.content_rect().y, -15.0);

        scroll.set_viewport(Rect::new(0.0, 10.0, 200.0, 300.0));
        assert_eq!(scroll.max_offset(), 0.0);
        assert_eq!(scroll.offset(), 0.0);
    }

    #[test]
    fn scroll_area_reports_partial_visibility() {
        let mut scroll = ScrollArea::new(Rect::new(0.0, 0.0, 100.0, 50.0));
        scroll.set_content_height(200.0);
        scroll.set_offset(20.0);

        assert_eq!(
            scroll.visible_rect(Rect::new(0.0, 0.0, 100.0, 10.0)),
            Some(Rect::new(0.0, 0.0, 100.0, 10.0))
        );
        assert!(scroll.is_visible(Rect::new(0.0, 40.0, 100.0, 10.0)));
        assert!(!scroll.is_visible(Rect::new(0.0, 50.0, 100.0, 10.0)));
        assert!(!scroll.is_visible(Rect::new(0.0, 0.0, 100.0, 0.0)));
    }

    #[test]
    fn scroll_content_layout_starts_at_the_scrolled_top() {
        let mut scroll = ScrollArea::new(Rect::new(10.0, 20.0, 80.0, 30.0));
        scroll.set_content_height(100.0);
        scroll.set_offset(15.0);
        let mut layout = scroll.content_layout().with_spacing(2.0);
        assert_eq!(layout.next_fixed(10.0), Rect::new(10.0, 5.0, 80.0, 10.0));
        assert_eq!(layout.next_fixed(10.0), Rect::new(10.0, 17.0, 80.0, 10.0));
    }
}
