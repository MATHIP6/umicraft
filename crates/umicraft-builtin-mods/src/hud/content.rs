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
    /// The look of the widget, set by the registry from the shared options.
    pub style: HudStyle,
    /// Whether the player asked to swap the side of the pictures: they stand
    /// where the client would not put them on its own (see `mirrored` in the UI).
    pub invert: bool,
}

impl HudContent {
    /// Forgets the lines and boxes but keeps their buffers.
    pub fn clear(&mut self) {
        self.used = 0;
        self.cells_used = 0;
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
        &mut cell.text
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

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;

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
