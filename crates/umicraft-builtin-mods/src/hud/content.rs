// SPDX-License-Identifier: MIT OR Apache-2.0
//! The text a HUD mod shows, in buffers that are reused from frame to frame.

/// One line of a widget.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HudLine {
    /// The text.
    pub text: String,
    /// Color as `0xRRGGBB`.
    pub color: u32,
}

/// The lines of one widget.
///
/// `clear` keeps the strings allocated, so after the first frames a mod writes
/// its lines without touching the allocator (`write!` into [`line`](Self::line)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HudContent {
    lines: Vec<HudLine>,
    used: usize,
}

impl HudContent {
    /// Forgets the lines but keeps their buffers.
    pub fn clear(&mut self) {
        self.used = 0;
    }

    /// Starts a new line of the given color and returns its (empty) text to write into.
    pub fn line(&mut self, color: u32) -> &mut String {
        if self.used == self.lines.len() {
            self.lines.push(HudLine::default());
        }
        let line = &mut self.lines[self.used];
        self.used += 1;
        line.text.clear();
        line.color = color;
        &mut line.text
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
}
