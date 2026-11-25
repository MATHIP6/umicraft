// SPDX-License-Identifier: MIT OR Apache-2.0
//! The text a HUD mod shows, in buffers that are reused from frame to frame.

use super::HudStyle;

/// The small picture drawn before a line of a widget. The mod only names it; the
/// client picks the sprite, so a mod never knows about textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudIcon {
    /// No picture.
    #[default]
    None,
    /// The armor worn in a slot, `0` the helmet to `3` the boots.
    Armor(u8),
    /// The item in the hand.
    Held,
    /// The status icon of the potion effect with this 1.8 id.
    Effect(u8),
}

/// One line of a widget.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HudLine {
    /// The picture before the text.
    pub icon: HudIcon,
    /// The text.
    pub text: String,
    /// Color as `0xRRGGBB`.
    pub color: u32,
    /// A smaller text under the main one (the time left of an effect); empty for none.
    pub detail: String,
    /// Color of the detail as `0xRRGGBB`.
    pub detail_color: u32,
}

/// A box of a widget drawn at a fixed place, such as a key of the keystrokes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HudCell {
    /// `[x, y, width, height]` in GUI units, from the top-left corner of the widget.
    pub rect: [f32; 4],
    /// Whether the box is lit (the key is held).
    pub pressed: bool,
    /// Color of the text as `0xRRGGBB`.
    pub color: u32,
    /// The text, centered in the box.
    pub text: String,
    /// The rectangle `[x, y, width, height]` it is cut to, `None` for none.
    pub clip: Option<[f32; 4]>,
}

/// A text placed freely in a widget, centered on a point and faded: the numbers
/// and letters of the direction HUD.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HudLabel {
    /// `[x, y]` in GUI units from the top-left of the widget: the center of the
    /// text horizontally and its top edge.
    pub at: [f32; 2],
    /// The text.
    pub text: String,
    /// Color as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`.
    pub alpha: f32,
    /// Size of the text relative to the normal one.
    pub scale: f32,
    /// The rectangle `[x, y, width, height]` it is cut to, `None` for none.
    pub clip: Option<[f32; 4]>,
}

/// A small filled rectangle of a widget, such as a mark of the direction HUD ruler.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HudTick {
    /// `[x, y, width, height]` in GUI units from the top-left of the widget.
    pub rect: [f32; 4],
    /// Color as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`.
    pub alpha: f32,
    /// The rectangle `[x, y, width, height]` it is cut to, `None` for none.
    pub clip: Option<[f32; 4]>,
}

/// A straight line of a widget, at any angle (the `hud_segment` of mods).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HudSegment {
    /// Start, in GUI units from the top-left of the widget.
    pub from: [f32; 2],
    /// End, in GUI units from the top-left of the widget.
    pub to: [f32; 2],
    /// Width, in GUI units.
    pub thickness: f32,
    /// Color as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`.
    pub alpha: f32,
    /// The rectangle `[x, y, width, height]` it is cut to, `None` for none.
    pub clip: Option<[f32; 4]>,
}

/// A circle of a widget, filled or a ring (the `hud_circle` of mods, API 1.4).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HudCircle {
    /// Centre, in GUI units from the top-left of the widget.
    pub center: [f32; 2],
    /// Radius, in GUI units.
    pub radius: f32,
    /// Width of the ring in GUI units; `0.0` fills the disc.
    pub thickness: f32,
    /// Color as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`.
    pub alpha: f32,
    /// The rectangle `[x, y, width, height]` it is cut to, `None` for none.
    pub clip: Option<[f32; 4]>,
}

/// A filled convex polygon of a widget (the `hud_polygon` of mods, API 1.4).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HudPolygon {
    /// The corners in order, in GUI units from the top-left of the widget.
    pub points: Vec<[f32; 2]>,
    /// Color as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`.
    pub alpha: f32,
    /// The rectangle `[x, y, width, height]` it is cut to, `None` for none.
    pub clip: Option<[f32; 4]>,
}

/// The lines and boxes of one widget.
///
/// `clear` keeps the strings allocated, so after the first frames a mod writes
/// its lines without touching the allocator (`write!` into [`line`](Self::line)).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudContent {
    lines: Vec<HudLine>,
    used: usize,
    cells: Vec<HudCell>,
    cells_used: usize,
    labels: Vec<HudLabel>,
    labels_used: usize,
    ticks: Vec<HudTick>,
    segments: Vec<HudSegment>,
    circles: Vec<HudCircle>,
    polygons: Vec<HudPolygon>,
    polygons_used: usize,
    /// The open clip rectangles, each already cut to the one before it.
    clips: Vec<[f32; 4]>,
    /// The look of the widget, set by the registry from the shared options.
    pub style: HudStyle,
    /// Whether the player asked to swap the side of the pictures: they stand
    /// where the client would not put them on its own (see `mirrored` in the UI).
    pub invert: bool,
    /// The size `[width, height]` in GUI units the widget keeps whatever it draws
    /// (`[0.0; 2]` for none), so a widget whose marks come and go does not
    /// change size and jump. Set by the mod on each draw.
    pub reserve: [f32; 2],
}

impl HudContent {
    /// Forgets the lines and boxes but keeps their buffers.
    pub fn clear(&mut self) {
        self.used = 0;
        self.cells_used = 0;
        self.labels_used = 0;
        self.ticks.clear();
        self.segments.clear();
        self.circles.clear();
        self.polygons_used = 0;
        self.clips.clear();
        self.reserve = [0.0; 2];
    }

    /// The clip the next free item gets: the innermost open one.
    #[must_use]
    pub fn clip(&self) -> Option<[f32; 4]> {
        self.clips.last().copied()
    }

    /// How many clips are open.
    #[must_use]
    pub fn clip_depth(&self) -> usize {
        self.clips.len()
    }

    /// Opens a clip: the free items added next are cut to `rect`
    /// (`[x, y, width, height]`) within the clip already open. An empty
    /// intersection hides them.
    pub fn push_clip(&mut self, rect: [f32; 4]) {
        let rect = match self.clip() {
            Some(outer) => intersect(outer, rect),
            None => rect,
        };
        self.clips.push(rect);
    }

    /// Closes the innermost clip; `false` when none is open.
    pub fn pop_clip(&mut self) -> bool {
        self.clips.pop().is_some()
    }

    /// Starts a new line of the given color and returns its (empty) text to write into.
    pub fn line(&mut self, color: u32) -> &mut String {
        &mut self.icon_line(HudIcon::None, color).text
    }

    /// Starts a new line with a picture before it. The text and the detail are empty
    /// and the detail has the same color as the text until the mod changes it.
    pub fn icon_line(&mut self, icon: HudIcon, color: u32) -> &mut HudLine {
        if self.used == self.lines.len() {
            self.lines.push(HudLine::default());
        }
        let line = &mut self.lines[self.used];
        self.used += 1;
        line.icon = icon;
        line.text.clear();
        line.color = color;
        line.detail.clear();
        line.detail_color = color;
        line
    }

    /// Starts a new box at `rect` and returns its (empty) text to write into.
    pub fn cell(&mut self, rect: [f32; 4], pressed: bool, color: u32) -> &mut String {
        if self.cells_used == self.cells.len() {
            self.cells.push(HudCell::default());
        }
        let cell = &mut self.cells[self.cells_used];
        self.cells_used += 1;
        cell.rect = rect;
        cell.pressed = pressed;
        cell.color = color;
        cell.text.clear();
        cell.clip = self.clips.last().copied();
        &mut cell.text
    }

    /// Starts a new free text centered on `at` and returns its (empty) text to
    /// write into.
    pub fn label(&mut self, at: [f32; 2], color: u32, alpha: f32, scale: f32) -> &mut String {
        if self.labels_used == self.labels.len() {
            self.labels.push(HudLabel::default());
        }
        let label = &mut self.labels[self.labels_used];
        self.labels_used += 1;
        label.at = at;
        label.color = color;
        label.alpha = alpha;
        label.scale = scale;
        label.text.clear();
        label.clip = self.clips.last().copied();
        &mut label.text
    }

    /// The free texts written since the last [`clear`](Self::clear).
    #[must_use]
    pub fn labels(&self) -> &[HudLabel] {
        &self.labels[..self.labels_used]
    }

    /// Adds a small filled rectangle.
    pub fn tick(&mut self, rect: [f32; 4], color: u32, alpha: f32) {
        let clip = self.clip();
        self.ticks.push(HudTick {
            rect,
            color,
            alpha,
            clip,
        });
    }

    /// The rectangles added since the last [`clear`](Self::clear).
    #[must_use]
    pub fn ticks(&self) -> &[HudTick] {
        &self.ticks
    }

    /// Adds a straight line, cut to the open clip.
    pub fn segment(&mut self, segment: HudSegment) {
        let clip = self.clip();
        self.segments.push(HudSegment { clip, ..segment });
    }

    /// The lines added since the last [`clear`](Self::clear).
    #[must_use]
    pub fn segments(&self) -> &[HudSegment] {
        &self.segments
    }

    /// Adds a circle, cut to the open clip.
    pub fn circle(&mut self, circle: HudCircle) {
        let clip = self.clip();
        self.circles.push(HudCircle { clip, ..circle });
    }

    /// The circles added since the last [`clear`](Self::clear).
    #[must_use]
    pub fn circles(&self) -> &[HudCircle] {
        &self.circles
    }

    /// Adds a filled convex polygon through `points`, cut to the open clip.
    pub fn polygon(&mut self, points: impl IntoIterator<Item = [f32; 2]>, color: u32, alpha: f32) {
        if self.polygons_used == self.polygons.len() {
            self.polygons.push(HudPolygon::default());
        }
        let clip = self.clip();
        let polygon = &mut self.polygons[self.polygons_used];
        self.polygons_used += 1;
        polygon.points.clear();
        polygon.points.extend(points);
        polygon.color = color;
        polygon.alpha = alpha;
        polygon.clip = clip;
    }

    /// The polygons added since the last [`clear`](Self::clear).
    #[must_use]
    pub fn polygons(&self) -> &[HudPolygon] {
        &self.polygons[..self.polygons_used]
    }

    /// Whether the widget shows nothing: no line, box, label, mark, segment,
    /// circle or polygon, and no reserved size.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.used == 0
            && self.cells_used == 0
            && self.labels_used == 0
            && self.ticks.is_empty()
            && self.segments.is_empty()
            && self.circles.is_empty()
            && self.polygons_used == 0
            && self.reserve == [0.0; 2]
    }

    /// The boxes written since the last [`clear`](Self::clear).
    #[must_use]
    pub fn cells(&self) -> &[HudCell] {
        &self.cells[..self.cells_used]
    }

    /// The lines written since the last [`clear`](Self::clear).
    #[must_use]
    pub fn lines(&self) -> &[HudLine] {
        &self.lines[..self.used]
    }
}

/// The part of `a` inside `b`, both `[x, y, width, height]`; zero-sized when they
/// do not overlap.
fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    [left, top, (right - left).max(0.0), (bottom - top).max(0.0)]
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;

    #[test]
    fn free_items_take_the_innermost_clip_and_clear_closes_them() {
        let mut c = HudContent::default();
        c.tick([0.0; 4], 0, 1.0);
        c.push_clip([0.0, 0.0, 10.0, 10.0]);
        c.push_clip([5.0, 5.0, 10.0, 10.0]);
        c.segment(HudSegment::default());
        c.circle(HudCircle::default());
        c.polygon([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], 0, 1.0);
        assert!(c.pop_clip());
        c.label([0.0; 2], 0, 1.0, 1.0).push('x');
        c.cell([0.0; 4], false, 0).push('y');
        assert_eq!(c.ticks()[0].clip, None);
        let inner = Some([5.0, 5.0, 5.0, 5.0]);
        assert_eq!(c.segments()[0].clip, inner);
        assert_eq!(c.circles()[0].clip, inner);
        assert_eq!(c.polygons()[0].clip, inner);
        assert_eq!(c.labels()[0].clip, Some([0.0, 0.0, 10.0, 10.0]));
        assert_eq!(c.cells()[0].clip, Some([0.0, 0.0, 10.0, 10.0]));
        c.clear();
        assert_eq!(c.clip_depth(), 0);
        assert!(!c.pop_clip());
        assert!(c.circles().is_empty() && c.polygons().is_empty() && c.is_empty());
    }

    #[test]
    fn disjoint_clips_leave_an_empty_rectangle() {
        let mut c = HudContent::default();
        c.push_clip([0.0, 0.0, 4.0, 4.0]);
        c.push_clip([10.0, 10.0, 4.0, 4.0]);
        assert_eq!(c.clip(), Some([10.0, 10.0, 0.0, 0.0]));
    }

    #[test]
    fn a_polygon_reuses_its_points_buffer() {
        let mut c = HudContent::default();
        c.polygon([[0.0; 2]; 16], 0, 1.0);
        let capacity = c.polygons[0].points.capacity();
        c.clear();
        c.polygon([[1.0; 2]; 3], 0, 1.0);
        assert_eq!(c.polygons()[0].points, [[1.0; 2]; 3]);
        assert_eq!(c.polygons[0].points.capacity(), capacity);
    }

    #[test]
    fn lines_are_kept_in_order() {
        let mut c = HudContent::default();
        c.line(0xff0000).push('a');
        write!(c.line(0x00ff00), "b{}", 2).expect("string write");
        let texts: Vec<_> = c
            .lines()
            .iter()
            .map(|l| (l.text.as_str(), l.color))
            .collect();
        assert_eq!(texts, [("a", 0xff0000), ("b2", 0x00ff00)]);
    }

    #[test]
    fn clear_hides_old_lines_and_reuses_buffers() {
        let mut c = HudContent::default();
        c.line(1).push_str("long enough to allocate");
        let capacity = c.lines[0].text.capacity();
        c.clear();
        assert!(c.lines().is_empty());
        c.line(2).push('x');
        assert_eq!(c.lines().len(), 1);
        assert_eq!(c.lines[0].text.capacity(), capacity);
        assert_eq!(c.lines()[0].text, "x", "the old text is gone");
    }

    #[test]
    fn an_icon_line_starts_clean_each_frame() {
        let mut c = HudContent::default();
        let line = c.icon_line(HudIcon::Held, 0x11_2233);
        line.text.push('a');
        line.detail.push('b');
        line.detail_color = 7;
        c.clear();
        let line = c.icon_line(HudIcon::Effect(3), 0x44_5566);
        assert_eq!(
            (line.icon, line.text.as_str(), line.detail.as_str()),
            (HudIcon::Effect(3), "", "")
        );
        assert_eq!((line.color, line.detail_color), (0x44_5566, 0x44_5566));
    }

    #[test]
    fn labels_ticks_and_the_reserve_are_cleared_with_the_rest() {
        let mut c = HudContent::default();
        c.label([10.0, 2.0], 0xff_ffff, 0.5, 1.5).push('S');
        c.tick([1.0, 2.0, 1.0, 3.0], 0xff_ffff, 1.0);
        c.segment(HudSegment {
            to: [4.0, 4.0],
            thickness: 1.0,
            ..HudSegment::default()
        });
        c.reserve = [100.0, 20.0];
        assert_eq!(
            (c.labels()[0].text.as_str(), c.labels()[0].scale),
            ("S", 1.5)
        );
        assert_eq!(c.ticks().len(), 1);
        c.clear();
        assert!(c.labels().is_empty() && c.ticks().is_empty() && c.segments().is_empty());
        assert!(c.is_empty());
        assert_eq!(
            c.label([0.0; 2], 0, 1.0, 1.0),
            "",
            "a reused label has no old text"
        );
    }

    #[test]
    fn cells_are_kept_in_order_and_cleared_with_the_lines() {
        let mut c = HudContent::default();
        c.cell([0.0, 0.0, 22.0, 22.0], true, 1).push('W');
        c.cell([24.0, 0.0, 22.0, 22.0], false, 2).push('A');
        let cells: Vec<_> = c
            .cells()
            .iter()
            .map(|k| (k.text.as_str(), k.pressed))
            .collect();
        assert_eq!(cells, [("W", true), ("A", false)]);
        c.clear();
        assert!(c.cells().is_empty());
        assert_eq!(
            c.cell([0.0; 4], false, 0),
            "",
            "a reused box has no old text"
        );
    }
}
