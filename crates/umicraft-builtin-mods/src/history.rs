// SPDX-License-Identifier: MIT OR Apache-2.0

//! The undo and redo of the HUD editor: the saved states of the mods before
//! each change, newest last.

use crate::config::ModsConfig;

/// How many changes can be undone; the oldest is forgotten beyond.
const LIMIT: usize = 64;

/// The states to go back to and forward to.
#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<ModsConfig>,
    redo: Vec<ModsConfig>,
    /// What the last recorded change was about, to merge a run of the same
    /// (the frames of one slider drag) into one step.
    last: String,
}

impl History {
    /// Records `before`, the state a change is about to replace. A change with
    /// the same non-empty `tag` as the previous one is part of it and adds no
    /// step. Whatever could be redone is forgotten.
    pub fn record(&mut self, before: ModsConfig, tag: &str) {
        self.redo.clear();
        if !tag.is_empty() && tag == self.last {
            return;
        }
        tag.clone_into(&mut self.last);
        if self.undo.len() == LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(before);
    }

    /// The state before the last change, `current` becoming what redo returns.
    pub fn undo(&mut self, current: ModsConfig) -> Option<ModsConfig> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.last.clear();
        Some(previous)
    }

    /// The state the last undo left, `current` becoming what undo returns.
    pub fn redo(&mut self, current: ModsConfig) -> Option<ModsConfig> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.last.clear();
        Some(next)
    }

    /// Whether there is a change to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is a change to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModConfig;

    /// A state told apart by whether `fps` is on.
    fn state(on: bool) -> ModsConfig {
        let mut config = ModsConfig::default();
        config.0.insert(
            "fps".into(),
            ModConfig {
                enabled: Some(on),
                ..ModConfig::default()
            },
        );
        config
    }

    #[test]
    fn undo_and_redo_walk_the_changes_both_ways() {
        let mut history = History::default();
        assert!(!history.can_undo() && !history.can_redo());
        history.record(state(false), "");
        assert_eq!(history.undo(state(true)), Some(state(false)));
        assert!(!history.can_undo() && history.can_redo());
        assert_eq!(history.redo(state(false)), Some(state(true)));
        assert!(history.can_undo() && !history.can_redo());
        assert_eq!(history.redo(state(true)), None);
    }

    #[test]
    fn a_new_change_forgets_what_could_be_redone() {
        let mut history = History::default();
        history.record(state(false), "");
        let _ = history.undo(state(true));
        history.record(state(false), "");
        assert!(!history.can_redo());
    }

    #[test]
    fn a_run_of_the_same_change_is_one_step() {
        let mut history = History::default();
        for _ in 0..30 {
            history.record(state(false), "fps/size");
        }
        history.record(state(true), "fps/color");
        assert!(history.undo(state(true)).is_some());
        assert!(history.undo(state(true)).is_some());
        assert!(history.undo(state(true)).is_none(), "two steps, not 31");
    }

    #[test]
    fn an_undo_ends_the_run() {
        let mut history = History::default();
        history.record(state(false), "fps/size");
        let _ = history.undo(state(true));
        history.record(state(false), "fps/size");
        assert!(history.can_undo(), "the same tag after an undo is a new step");
    }

    #[test]
    fn the_oldest_change_is_forgotten_beyond_the_limit() {
        let mut history = History::default();
        for _ in 0..LIMIT + 10 {
            history.record(state(false), "");
        }
        let undone = std::iter::from_fn(|| history.undo(state(true))).count();
        assert_eq!(undone, LIMIT);
    }
}
