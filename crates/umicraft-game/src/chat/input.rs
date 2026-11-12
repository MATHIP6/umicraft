// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat input line: editing, the history of sent messages and the Tab
//! completion (M10, GAM-6).
//!
//! The behaviour follows MCP `GuiChat` (`keyTyped`, `getSentHistory`,
//! `autocompletePlayerNames`, `onAutocompleteResponse`) and `GuiTextField`
//! (`writeText`, `func_146197_a` word movement, `deleteFromCursor`, the
//! selection kept as `cursorPosition` and `selectionEnd`). The type is
//! I/O-free: the window feeds it keys, owns the clipboard and sends the
//! requests it returns.

use std::time::Instant;

use umicraft_core::text_edit::{
    word_boundary, ClickSeries, EditKind, History, Selection, Snapshot,
};

use super::commands::MAX_LEN;
use super::complete::{common_prefix, Completion};
use super::history::SentHistory;

/// What pressing Tab asks the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabOutcome {
    /// Nothing: the line is empty.
    Nothing,
    /// Send a tab-complete request with this text (the line up to the caret).
    Request(String),
    /// A completion was written into the line. When the server offered
    /// several, `listing` is the comma-separated list to print in the chat.
    Cycled {
        /// The matches to print locally, when there is more than one.
        listing: Option<String>,
    },
}

/// The undo kind of a deletion: single characters join into one step, a word
/// or a selection is a step of its own.
fn delete_kind(selected: bool, word: bool) -> EditKind {
    if selected || word {
        EditKind::Other
    } else {
        EditKind::Deleting
    }
}

/// The chat input line.
#[derive(Debug, Clone)]
pub struct ChatInput {
    text: Vec<char>,
    selection: Selection,
    completion: Completion,
    history: History,
    clicks: ClickSeries,
    /// The primary button went down on the line and is still held.
    dragging: bool,
}

impl ChatInput {
    /// A line holding `initial` (a leading `/` when the command key opened
    /// the chat).
    #[must_use]
    pub fn new(initial: &str) -> Self {
        let mut input = Self {
            text: Vec::new(),
            selection: Selection::default(),
            completion: Completion::default(),
            history: History::default(),
            clicks: ClickSeries::default(),
            dragging: false,
        };
        input.write(initial);
        input
    }

    /// The text of the line.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// The caret as a character index, `0..=len`.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.selection.caret()
    }

    /// The selected `char` range, start before end, or `None` when nothing is
    /// selected.
    #[must_use]
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.selection
            .has_selection()
            .then(|| self.selection.range())
    }

    /// The selected text, empty when nothing is selected.
    #[must_use]
    pub fn selected_text(&self) -> String {
        let (start, end) = self.selection.range();
        self.text[start..end].iter().collect()
    }

    /// The number of characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether the line is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Any edit other than Tab ends the completion (`keyTyped` resets it).
    fn interrupt(&mut self) {
        self.completion.interrupt();
    }

    /// Writes at the caret, dropping the characters the chat does not accept
    /// (control characters and the section sign) and stopping at the limit.
    fn write(&mut self, text: &str) {
        for character in text.chars() {
            if self.text.len() >= MAX_LEN {
                break;
            }
            if character < ' ' || character == '\u{7f}' || character == '\u{a7}' {
                continue;
            }
            let caret = self.caret();
            self.text.insert(caret, character);
            self.selection.collapse(caret + 1, self.text.len());
        }
    }

    /// Removes the selected text and puts the caret where it was. Returns
    /// whether something was removed.
    fn remove_selection(&mut self) -> bool {
        let (start, end) = self.selection.range();
        self.text.drain(start..end);
        self.selection.collapse(start, self.text.len());
        start != end
    }

    /// Removes `start..end` and puts the caret at `start`.
    fn remove_range(&mut self, start: usize, end: usize) {
        self.text.drain(start..end);
        self.selection.collapse(start, self.text.len());
    }

    /// Types text at the caret, replacing the selection (`writeText`).
    pub fn insert(&mut self, text: &str) {
        self.interrupt();
        let kind = if self.selection.has_selection() {
            EditKind::Other
        } else {
            EditKind::Typing
        };
        self.record(kind);
        self.remove_selection();
        self.write(text);
    }

    /// Pastes `text` at the caret: an undo step of its own.
    pub fn paste(&mut self, text: &str) {
        self.interrupt();
        self.record(EditKind::Other);
        self.remove_selection();
        self.write(text);
    }

    /// Notes the line before an edit of `kind` for the undo.
    fn record(&mut self, kind: EditKind) {
        let before = self.snapshot();
        self.history.record(kind, || before);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text(),
            selection: self.selection,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text.chars().collect();
        self.selection = snapshot.selection;
        self.selection.clamp(self.text.len());
    }

    /// Undoes the last editing step (Control+Z).
    pub fn undo(&mut self) {
        self.interrupt();
        let current = self.snapshot();
        if let Some(previous) = self.history.undo(current) {
            self.restore(previous);
        }
    }

    /// Redoes the last undone step (Control+Y).
    pub fn redo(&mut self) {
        self.interrupt();
        let current = self.snapshot();
        if let Some(next) = self.history.redo(current) {
            self.restore(next);
        }
    }

    /// Selects the whole line, the caret at the start.
    pub fn select_all(&mut self) {
        self.interrupt();
        self.selection.select_all(self.text.len());
    }

    /// Removes the selection and returns it, for Control+X.
    pub fn cut(&mut self) -> String {
        self.interrupt();
        let cut = self.selected_text();
        if self.selection.has_selection() {
            self.record(EditKind::Other);
        }
        self.remove_selection();
        cut
    }

    /// Replaces the whole line and puts the caret at its end.
    pub fn set_text(&mut self, text: &str) {
        self.interrupt();
        self.text.clear();
        self.selection = Selection::default();
        self.history.clear();
        self.write(text);
    }

    /// The caret position one word away (`func_146197_a`): a word move skips
    /// the spaces next to the caret, then the word.
    fn word_boundary(&self, backwards: bool, skip_spaces: bool) -> usize {
        word_boundary(&self.text, self.caret(), !backwards, skip_spaces)
    }

    /// Deletes the selection, else the character before the caret, or the
    /// word before it.
    pub fn backspace(&mut self, word: bool) {
        self.interrupt();
        if self.selection.has_selection() || self.caret() > 0 {
            self.record(delete_kind(self.selection.has_selection(), word));
        }
        if self.remove_selection() || self.caret() == 0 {
            return;
        }
        let start = if word {
            self.word_boundary(true, true)
        } else {
            self.caret() - 1
        };
        self.remove_range(start, self.caret());
    }

    /// Deletes the selection, else the character after the caret, or the word
    /// after it.
    pub fn delete(&mut self, word: bool) {
        self.interrupt();
        if self.selection.has_selection() || self.caret() < self.text.len() {
            self.record(delete_kind(self.selection.has_selection(), word));
        }
        if self.remove_selection() || self.caret() >= self.text.len() {
            return;
        }
        let end = if word {
            self.word_boundary(false, true)
        } else {
            self.caret() + 1
        };
        self.remove_range(self.caret(), end);
    }

    /// Moves the caret to `target`; `extend` (Shift) grows the selection
    /// instead of dropping it.
    fn move_caret(&mut self, target: usize, extend: bool) {
        self.interrupt();
        self.history.break_run();
        self.selection.move_to(target, self.text.len(), extend);
    }

    /// A primary press at the character boundary `index`: Shift extends the
    /// selection; otherwise a click places the caret, a double click selects
    /// the word and a triple click the line.
    pub fn press(&mut self, index: usize, shift: bool, now: Instant) {
        self.interrupt();
        self.history.break_run();
        let rank = self.clicks.register(now, index);
        let len = self.text.len();
        if shift {
            self.selection.extend_to(index, len);
        } else {
            match rank {
                1 => self.selection.collapse(index, len),
                2 => self.select_word_at(index),
                _ => self.selection.select_all(len),
            }
        }
        self.dragging = !shift && rank == 1;
    }

    /// The pointer moved to the boundary `index` with the button held: the
    /// selection follows it. Does nothing when no press started on the line.
    pub fn drag_to(&mut self, index: usize) {
        if self.dragging {
            self.selection.extend_to(index, self.text.len());
        }
    }

    /// The primary button went up: the drag ends.
    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// Selects the word around `at`; on spaces only the caret moves.
    fn select_word_at(&mut self, at: usize) {
        let len = self.text.len();
        let at = at.min(len);
        let probe = if self.text.get(at).is_some_and(|c| *c != ' ') {
            at
        } else if at > 0 && self.text[at - 1] != ' ' {
            at - 1
        } else {
            self.selection.collapse(at, len);
            return;
        };
        let start = word_boundary(&self.text, probe + 1, false, false);
        let end = word_boundary(&self.text, probe, true, false);
        self.selection.select_range(start, end);
    }

    /// Moves the caret one character or one word left.
    pub fn left(&mut self, word: bool, extend: bool) {
        let target = if word {
            self.word_boundary(true, true)
        } else {
            self.caret().saturating_sub(1)
        };
        self.move_caret(target, extend);
    }

    /// Moves the caret one character or one word right.
    pub fn right(&mut self, word: bool, extend: bool) {
        let target = if word {
            self.word_boundary(false, true)
        } else {
            self.caret() + 1
        };
        self.move_caret(target, extend);
    }

    /// Moves the caret to the start of the line.
    pub fn home(&mut self, extend: bool) {
        self.move_caret(0, extend);
    }

    /// Moves the caret to the end of the line.
    pub fn end(&mut self, extend: bool) {
        self.move_caret(self.text.len(), extend);
    }

    /// Shows the previous sent message (the up key).
    pub fn history_previous(&mut self, history: &mut SentHistory) {
        self.browse_history(-1, history);
    }

    /// Shows the next sent message, then the line being typed (the down key).
    pub fn history_next(&mut self, history: &mut SentHistory) {
        self.browse_history(1, history);
    }

    fn browse_history(&mut self, delta: isize, history: &mut SentHistory) {
        self.interrupt();
        if let Some(text) = history.browse(delta, &self.text()) {
            self.set_text(&text);
        }
    }

    /// The start of the word before the caret (`func_146197_a(-1, …, false)`).
    fn word_start(&self) -> usize {
        self.word_boundary(true, false)
    }

    /// Presses Tab: asks the server for completions, or cycles through the
    /// ones it already offered.
    pub fn tab(&mut self) -> TabOutcome {
        self.completion.waiting = false;
        if self.completion.cycling {
            return TabOutcome::Cycled {
                listing: self.write_next_match(),
            };
        }
        self.completion.clear_matches();
        if self.caret() == 0 {
            return TabOutcome::Nothing;
        }
        self.completion.waiting = true;
        TabOutcome::Request(self.text[..self.caret()].iter().collect())
    }

    /// Replaces the word before the caret with the next match and returns the
    /// list to print when there are several.
    fn write_next_match(&mut self) -> Option<String> {
        self.remove_range(self.word_start(), self.caret());
        let (name, listing) = self.completion.next_match();
        if let Some(name) = name {
            self.write(&name);
        }
        listing
    }

    /// Handles the server's completions. A longer common prefix is written
    /// first; when the word already is that prefix, Tab starts cycling.
    /// Returns the list to print when the first match is written and there
    /// are several. Answers that arrive after another key are ignored.
    pub fn on_completions(&mut self, matches: &[String]) -> Option<String> {
        if !self.completion.waiting {
            return None;
        }
        self.completion.cycling = false;
        self.completion.matches = matches
            .iter()
            .filter(|candidate| !candidate.is_empty())
            .cloned()
            .collect();
        let word: String = self.text[self.word_start()..self.caret()].iter().collect();
        let prefix = common_prefix(matches);
        if !prefix.is_empty() && !word.eq_ignore_ascii_case(&prefix) {
            self.remove_range(self.word_start(), self.caret());
            self.write(&prefix);
            None
        } else if !self.completion.matches.is_empty() {
            self.completion.cycling = true;
            self.write_next_match()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn typed(text: &str) -> ChatInput {
        let mut input = ChatInput::new("");
        input.insert(text);
        input
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut input = typed("helo");
        input.left(false, false);
        input.insert("l");
        assert_eq!(input.text(), "hello");
        assert_eq!(input.caret(), 4);
    }

    #[test]
    fn the_line_is_capped_at_one_hundred_characters() {
        let input = typed(&"a".repeat(150));
        assert_eq!(input.len(), MAX_LEN);
    }

    #[test]
    fn control_characters_and_the_section_sign_are_dropped() {
        let input = typed("a\nb\tc\u{a7}d\u{7f}e");
        assert_eq!(input.text(), "abcde");
    }

    #[test]
    fn characters_not_bytes_are_counted() {
        let input = typed("\u{e9}\u{e9}\u{e9}");
        assert_eq!(input.len(), 3);
        assert_eq!(input.caret(), 3);
    }

    #[test]
    fn backspace_and_delete_remove_one_character_or_one_word() {
        let mut input = typed("one two three");
        input.backspace(false);
        assert_eq!(input.text(), "one two thre");
        input.backspace(true);
        assert_eq!(input.text(), "one two ");
        input.home(false);
        input.delete(false);
        assert_eq!(input.text(), "ne two ");
        input.delete(true);
        assert_eq!(input.text(), "two ");
        // At the edges nothing happens and nothing panics.
        input.home(false);
        input.backspace(false);
        input.end(false);
        input.delete(false);
        assert_eq!(input.text(), "two ");
    }

    #[test]
    fn the_caret_moves_by_characters_words_and_to_the_edges() {
        let mut input = typed("ab cd ef");
        input.left(true, false);
        assert_eq!(input.caret(), 6);
        input.left(true, false);
        assert_eq!(input.caret(), 3);
        input.right(true, false);
        assert_eq!(input.caret(), 6);
        input.home(false);
        input.left(false, false);
        assert_eq!(input.caret(), 0);
        input.end(false);
        input.right(false, false);
        assert_eq!(input.caret(), 8);
    }

    #[test]
    fn shift_movement_selects_by_character_word_and_to_the_edges() {
        let mut input = typed("ab cd ef");
        input.left(false, true);
        assert_eq!(input.selection(), Some((7, 8)));
        input.left(true, true);
        assert_eq!(input.selected_text(), "ef");
        input.home(true);
        assert_eq!(input.selected_text(), "ab cd ef");
        input.right(true, true);
        assert_eq!(
            input.selected_text(),
            "cd ef",
            "the anchor stays at the end"
        );
        input.end(false);
        assert_eq!(input.selection(), None, "a plain move drops the selection");
        assert_eq!(input.caret(), 8);
    }

    #[test]
    fn select_all_covers_the_whole_line() {
        let mut input = typed("hello");
        input.select_all();
        assert_eq!(input.selected_text(), "hello");
        assert_eq!(input.caret(), 0);
    }

    #[test]
    fn typing_or_pasting_replaces_the_selection() {
        let mut input = typed("hello world");
        input.left(true, true);
        input.insert("there");
        assert_eq!(input.text(), "hello there");
        assert_eq!(input.selection(), None);
        input.select_all();
        input.insert("bye");
        assert_eq!(input.text(), "bye");
        assert_eq!(input.caret(), 3);
    }

    #[test]
    fn backspace_and_delete_remove_the_selection_not_a_character() {
        let mut input = typed("one two three");
        input.left(true, true);
        input.backspace(false);
        assert_eq!(input.text(), "one two ");
        input.left(true, true);
        input.delete(true);
        assert_eq!(input.text(), "one ");
    }

    #[test]
    fn cut_returns_and_removes_the_selection() {
        let mut input = typed("abc def");
        input.left(true, true);
        assert_eq!(input.cut(), "def");
        assert_eq!(input.text(), "abc ");
        assert_eq!(input.cut(), "", "nothing selected, nothing cut");
        assert_eq!(input.text(), "abc ");
    }

    #[test]
    fn replacing_a_selection_still_respects_the_length_limit() {
        let mut input = typed(&"a".repeat(MAX_LEN));
        input.select_all();
        input.insert(&"b".repeat(MAX_LEN + 20));
        assert_eq!(input.len(), MAX_LEN);
        assert!(input.text().chars().all(|c| c == 'b'));
    }

    #[test]
    fn selections_count_characters_not_bytes() {
        let mut input = typed("\u{e9}t\u{e9}");
        input.left(false, true);
        input.left(false, true);
        assert_eq!(input.selected_text(), "t\u{e9}");
    }

    #[test]
    fn the_history_keys_swap_the_line_and_return_to_the_unsent_draft() {
        let mut sent = SentHistory::new();
        sent.push("first");
        sent.push("second");
        let mut input = ChatInput::new("");
        input.insert("draft");
        input.history_previous(&mut sent);
        assert_eq!(input.text(), "second");
        assert_eq!(input.caret(), 6, "the caret goes to the end");
        input.history_previous(&mut sent);
        assert_eq!(input.text(), "first");
        input.history_next(&mut sent);
        input.history_next(&mut sent);
        assert_eq!(input.text(), "draft");
    }

    #[test]
    fn tab_on_an_empty_line_asks_for_nothing() {
        let mut input = ChatInput::new("");
        assert_eq!(input.tab(), TabOutcome::Nothing);
    }

    #[test]
    fn tab_asks_the_server_with_the_text_before_the_caret() {
        let mut input = typed("/tp St");
        assert_eq!(input.tab(), TabOutcome::Request("/tp St".to_string()));
        input.left(false, false);
        input.left(false, false);
        assert_eq!(input.tab(), TabOutcome::Request("/tp ".to_string()));
    }

    #[test]
    fn a_common_prefix_is_written_before_any_cycling() {
        let mut input = typed("/tp St");
        input.tab();
        let listing = input.on_completions(&names(&["Steve", "Stella"]));
        assert_eq!(listing, None);
        assert_eq!(input.text(), "/tp Ste");
    }

    #[test]
    fn tab_then_cycles_through_the_matches_and_lists_them_once_per_press() {
        let mut input = typed("/tp Ste");
        input.tab();
        // "Ste" already is the common prefix of both matches: cycle.
        let first = input.on_completions(&names(&["Steve", "Stella"]));
        assert_eq!(first.as_deref(), Some("Steve, Stella"));
        assert_eq!(input.text(), "/tp Steve");

        assert_eq!(
            input.tab(),
            TabOutcome::Cycled {
                listing: Some("Steve, Stella".to_string())
            }
        );
        assert_eq!(input.text(), "/tp Stella");
        input.tab();
        assert_eq!(input.text(), "/tp Steve", "the cycle wraps around");
    }

    #[test]
    fn a_single_match_is_completed_without_a_listing() {
        let mut input = typed("hi Al");
        input.tab();
        let listing = input.on_completions(&names(&["Alex"]));
        assert_eq!(listing, None);
        assert_eq!(input.text(), "hi Alex");
    }

    #[test]
    fn any_other_key_ends_the_cycle_and_drops_late_answers() {
        let mut input = typed("/tp Ste");
        input.tab();
        input.on_completions(&names(&["Steve", "Stella"]));
        input.insert("!");
        match input.tab() {
            TabOutcome::Request(_) => {}
            other => panic!("typing must restart the completion, got {other:?}"),
        }

        let mut late = typed("/tp Ste");
        late.tab();
        late.insert("v");
        assert_eq!(late.on_completions(&names(&["Steve"])), None);
        assert_eq!(late.text(), "/tp Stev", "a stale answer changes nothing");
    }

    #[test]
    fn an_answer_nobody_asked_for_is_ignored() {
        let mut input = typed("abc");
        assert_eq!(input.on_completions(&names(&["abcdef"])), None);
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn empty_matches_are_skipped_and_an_empty_answer_changes_nothing() {
        let mut input = typed("ab");
        input.tab();
        assert_eq!(input.on_completions(&names(&[])), None);
        assert_eq!(input.text(), "ab");

        let mut input = typed("ab");
        input.tab();
        // An empty string in the answer makes the common prefix empty.
        input.on_completions(&names(&["", "abc", "abd"]));
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn a_completion_respects_the_length_limit() {
        let mut input = typed(&format!("{} a", "b".repeat(96)));
        input.tab();
        input.on_completions(&names(&["a-very-long-name"]));
        assert!(input.len() <= MAX_LEN);
    }

    #[test]
    fn undo_and_redo_step_through_typing_runs_and_deletions() {
        let mut input = ChatInput::new("");
        input.insert("a");
        input.insert("b");
        input.left(false, false);
        input.insert("X");
        assert_eq!(input.text(), "aXb");
        input.undo();
        assert_eq!(input.text(), "ab", "the caret move ended the first run");
        input.undo();
        assert_eq!(input.text(), "");
        input.redo();
        assert_eq!(input.text(), "ab");
        input.end(false);
        input.backspace(false);
        input.backspace(false);
        assert_eq!(input.text(), "");
        input.undo();
        assert_eq!(input.text(), "ab", "two backspaces are one step");
    }

    #[test]
    fn pasting_and_cutting_are_steps_of_their_own() {
        let mut input = typed("abc");
        input.paste("def");
        input.select_all();
        assert_eq!(input.cut(), "abcdef");
        input.undo();
        assert_eq!(input.text(), "abcdef");
        assert_eq!(input.selection(), Some((0, 6)), "the selection comes back");
        input.undo();
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn a_new_edit_after_an_undo_drops_the_redo() {
        let mut input = typed("ab");
        input.undo();
        input.insert("c");
        input.redo();
        assert_eq!(input.text(), "c");
    }

    #[test]
    fn pasting_filters_like_typing() {
        let mut input = typed("a");
        input.paste("b\nc");
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn clicks_place_the_caret_then_select_a_word_then_the_line() {
        let start = Instant::now();
        let mut input = typed("one two three");
        input.press(5, false, start);
        assert_eq!((input.caret(), input.selection()), (5, None));
        input.release();
        input.press(5, false, start);
        assert_eq!(input.selected_text(), "two");
        input.press(5, false, start);
        assert_eq!(input.selected_text(), "one two three");
    }

    #[test]
    fn dragging_extends_until_the_release_and_shift_click_extends() {
        let start = Instant::now();
        let mut input = typed("abcdef");
        input.press(1, false, start);
        input.drag_to(4);
        assert_eq!(input.selected_text(), "bcd");
        input.release();
        input.drag_to(6);
        assert_eq!(input.selected_text(), "bcd", "no drag after the release");
        input.press(6, true, start);
        assert_eq!(input.selected_text(), "bcdef");
    }
}
