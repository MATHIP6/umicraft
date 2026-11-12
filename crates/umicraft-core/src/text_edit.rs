// SPDX-License-Identifier: MIT OR Apache-2.0

//! Caret, selection and word movement shared by every single-line text input
//! (the chat line and the UI text field).
//!
//! The behaviour follows MCP `GuiTextField` (`cursorPosition` and
//! `selectionEnd`, `getNthWordFromPos`). Positions are `char` indices, never
//! byte offsets, so an edit can only fall on a valid UTF-8 boundary.
//!
//! The module owns no text: callers keep their own buffer and pass its length
//! (or its characters) where a bound is needed.

/// A caret and the fixed end of the selection it extends.
///
/// `anchor == caret` means nothing is selected. The caret is the end that
/// moves, so Shift+arrow keeps the anchor and drags the caret
/// (`GuiTextField.setSelectionPos`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    caret: usize,
    anchor: usize,
}

impl Selection {
    /// An empty selection with the caret at `caret`.
    #[must_use]
    pub const fn at(caret: usize) -> Self {
        Self {
            caret,
            anchor: caret,
        }
    }

    /// The caret, a `char` index in `0..=len`.
    #[must_use]
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// Whether some text is selected.
    #[must_use]
    pub const fn has_selection(&self) -> bool {
        self.anchor != self.caret
    }

    /// The selected `char` range, start before end (empty when nothing is
    /// selected).
    #[must_use]
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.caret), self.anchor.max(self.caret))
    }

    /// Moves the caret to `caret` (clamped to `len`) and drops the selection.
    pub fn collapse(&mut self, caret: usize, len: usize) {
        self.caret = caret.min(len);
        self.anchor = self.caret;
    }

    /// Moves the caret to `caret` (clamped to `len`), keeping the anchor.
    pub fn extend_to(&mut self, caret: usize, len: usize) {
        self.caret = caret.min(len);
    }

    /// Moves the caret, extending the selection when `extend` is set (Shift).
    ///
    /// Returns whether the caret or the selection changed.
    pub fn move_to(&mut self, caret: usize, len: usize, extend: bool) -> bool {
        let before = *self;
        if extend {
            self.extend_to(caret, len);
        } else {
            self.collapse(caret, len);
        }
        *self != before
    }

    /// Selects everything, the caret at the start
    /// (`setCursorPositionEnd` then `setSelectionPos(0)`).
    pub fn select_all(&mut self, len: usize) {
        self.anchor = len;
        self.caret = 0;
    }

    /// Selects `start..end`, the caret at the end (a double click on a word).
    pub fn select_range(&mut self, start: usize, end: usize) {
        self.anchor = start;
        self.caret = end;
    }

    /// Selects from the caret back to `anchor`, used to delete one unit
    /// without a selection (`GuiTextField.deleteFromCursor`).
    pub fn anchor_at(&mut self, anchor: usize) {
        self.anchor = anchor;
    }

    /// Keeps the caret and the anchor inside a text that may have changed.
    pub fn clamp(&mut self, len: usize) {
        self.caret = self.caret.min(len);
        self.anchor = self.anchor.min(len);
    }
}

/// The caret position one word away from `from` (`getNthWordFromPos`).
///
/// A word ends at a space. Going backwards, the spaces next to `from` are
/// skipped first when `skip_spaces` is set, then the word. Going forwards,
/// the word is skipped, then the spaces when `skip_spaces` is set.
#[must_use]
pub fn word_boundary(chars: &[char], from: usize, forward: bool, skip_spaces: bool) -> usize {
    let mut at = from.min(chars.len());
    if forward {
        while at < chars.len() && chars[at] != ' ' {
            at += 1;
        }
        while skip_spaces && at < chars.len() && chars[at] == ' ' {
            at += 1;
        }
    } else {
        while skip_spaces && at > 0 && chars[at - 1] == ' ' {
            at -= 1;
        }
        while at > 0 && chars[at - 1] != ' ' {
            at -= 1;
        }
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn a_collapsed_selection_selects_nothing() {
        let selection = Selection::at(3);
        assert!(!selection.has_selection());
        assert_eq!(selection.range(), (3, 3));
    }

    #[test]
    fn extending_keeps_the_anchor_and_orders_the_range() {
        let mut selection = Selection::at(5);
        assert!(selection.move_to(2, 8, true));
        assert_eq!(selection.range(), (2, 5));
        assert_eq!(selection.caret(), 2);
        assert!(selection.move_to(7, 8, true));
        assert_eq!(selection.range(), (5, 7));
    }

    #[test]
    fn moving_without_extending_drops_the_selection() {
        let mut selection = Selection::at(1);
        selection.move_to(4, 8, true);
        assert!(selection.move_to(4, 8, false), "dropping it is a change");
        assert!(!selection.has_selection());
        assert!(!selection.move_to(4, 8, false), "nothing moved");
    }

    #[test]
    fn a_range_selection_puts_the_caret_at_its_end() {
        let mut selection = Selection::default();
        selection.select_range(2, 5);
        assert_eq!(selection.range(), (2, 5));
        assert_eq!(selection.caret(), 5);
    }

    #[test]
    fn positions_are_clamped_to_the_length() {
        let mut selection = Selection::at(0);
        selection.collapse(99, 4);
        assert_eq!(selection.caret(), 4);
        selection.select_all(4);
        selection.clamp(2);
        assert_eq!(selection.range(), (0, 2));
    }

    #[test]
    fn words_end_at_spaces_in_both_directions() {
        let text = chars("ab  cd ef");
        assert_eq!(word_boundary(&text, 9, false, true), 7);
        assert_eq!(word_boundary(&text, 7, false, true), 4);
        assert_eq!(word_boundary(&text, 0, true, true), 4);
        assert_eq!(word_boundary(&text, 4, true, true), 7);
        assert_eq!(word_boundary(&text, 7, true, true), 9);
    }

    #[test]
    fn without_skipping_a_backward_move_stops_after_the_space() {
        let text = chars("hi Al");
        assert_eq!(word_boundary(&text, 5, false, false), 3);
        assert_eq!(word_boundary(&text, 3, false, false), 3);
    }
}
