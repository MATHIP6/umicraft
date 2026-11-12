// SPDX-License-Identifier: MIT OR Apache-2.0

//! Undo and redo for the text field (Control+Z, Control+Y).
//!
//! Vanilla `GuiTextField` has no undo; this is a Umicraft addition. A run of
//! typed characters, or of deletions, is one step, so Control+Z removes a
//! word's worth of typing rather than a single letter. Moving the caret, a
//! paste or an edit of another kind ends the run.

use umicraft_core::text_edit::Selection;

/// Most steps kept; the oldest is dropped beyond it.
const MAX_STEPS: usize = 100;

/// What an edit did, which decides whether it joins the previous step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKind {
    /// Characters typed at the caret.
    Typing,
    /// Backspace or Delete of single characters.
    Deleting,
    /// Anything else (paste, cut, replacing a selection, deleting a word):
    /// always a step of its own.
    Other,
}

/// The text and caret at one moment.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Snapshot {
    pub text: String,
    pub selection: Selection,
}

impl Snapshot {
    /// The state of a field: its `text` and its caret with the selection.
    pub fn of(text: &str, selection: Selection) -> Self {
        Self {
            text: text.to_string(),
            selection,
        }
    }
}

/// The undo and redo stacks of one field.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<EditKind>,
}

impl History {
    /// Notes that `before` is about to change by an edit of `kind`.
    pub fn record(&mut self, kind: EditKind, before: impl FnOnce() -> Snapshot) {
        let joins = kind != EditKind::Other && self.last == Some(kind);
        if !joins {
            if self.undo.len() == MAX_STEPS {
                self.undo.remove(0);
            }
            self.undo.push(before());
        }
        self.redo.clear();
        self.last = Some(kind);
    }

    /// Ends the current run: the next edit starts a new step.
    pub fn break_run(&mut self) {
        self.last = None;
    }

    /// Forgets every step (the text was replaced from outside).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Steps back; `current` becomes redoable. `None` when nothing to undo.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.last = None;
        Some(previous)
    }

    /// Steps forward again; `current` becomes undoable.
    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.last = None;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Snapshot {
        Snapshot {
            text: text.to_string(),
            selection: Selection::default(),
        }
    }

    #[test]
    fn a_typing_run_is_one_step() {
        let mut history = History::default();
        history.record(EditKind::Typing, || at(""));
        history.record(EditKind::Typing, || at("a"));
        history.record(EditKind::Typing, || at("ab"));
        assert_eq!(history.undo(at("abc")), Some(at("")));
        assert_eq!(history.undo(at("")), None);
    }

    #[test]
    fn other_edits_and_broken_runs_start_new_steps() {
        let mut history = History::default();
        history.record(EditKind::Typing, || at(""));
        history.break_run();
        history.record(EditKind::Typing, || at("a"));
        history.record(EditKind::Other, || at("ab"));
        history.record(EditKind::Other, || at("abc"));
        assert_eq!(history.undo(at("abcd")), Some(at("abc")));
        assert_eq!(history.undo(at("abc")), Some(at("ab")));
        assert_eq!(history.undo(at("ab")), Some(at("a")));
        assert_eq!(history.undo(at("a")), Some(at("")));
    }

    #[test]
    fn redo_replays_undone_steps_and_a_new_edit_drops_them() {
        let mut history = History::default();
        history.record(EditKind::Other, || at("a"));
        let back = history.undo(at("ab")).expect("one step");
        assert_eq!(back, at("a"));
        assert_eq!(history.redo(back), Some(at("ab")));
        history.undo(at("ab"));
        history.record(EditKind::Other, || at("a"));
        assert_eq!(history.redo(at("x")), None);
    }

    #[test]
    fn only_the_newest_steps_are_kept() {
        let mut history = History::default();
        for step in 0..MAX_STEPS + 10 {
            history.record(EditKind::Other, || at(&step.to_string()));
        }
        let mut undone = 0;
        while history.undo(at("")).is_some() {
            undone += 1;
        }
        assert_eq!(undone, MAX_STEPS);
    }
}
