// SPDX-License-Identifier: MIT OR Apache-2.0

//! The history of sent messages, browsed with the up and down keys.
//!
//! Sources: MCP `GuiNewChat.addToSentMessages` (a message equal to the last
//! one is not added again), `GuiChat.initGui` (the cursor starts after the
//! newest message) and `GuiChat.getSentHistory` (the cursor is clamped to
//! `0..=len`; leaving the newest entry saves the line being typed in
//! `historyBuffer`, and coming back to it restores that line). The reference
//! list is unbounded; Umicraft keeps the newest [`MAX_HISTORY`] messages.

/// The sent messages kept for the up and down keys.
pub const MAX_HISTORY: usize = 100;

/// The messages the player sent, oldest first, and the browsing position.
#[derive(Debug, Clone, Default)]
pub struct SentHistory {
    messages: Vec<String>,
    /// Which message the line shows; `messages.len()` is "none, the draft".
    cursor: usize,
    /// The line being typed, kept while older messages are shown.
    buffer: String,
}

impl SentHistory {
    /// An empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The messages, oldest first.
    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Adds a sent message: a repeat of the last one is ignored and the
    /// oldest message goes first beyond [`MAX_HISTORY`].
    pub fn push(&mut self, message: &str) {
        if self.messages.last().map(String::as_str) != Some(message) {
            self.messages.push(message.to_string());
        }
        if self.messages.len() > MAX_HISTORY {
            self.messages.remove(0);
        }
        self.reset();
    }

    /// Puts the cursor after the newest message, as opening the chat does.
    pub fn reset(&mut self) {
        self.cursor = self.messages.len();
        self.buffer.clear();
    }

    /// Moves the cursor by `delta` (`-1` is the previous message) and returns
    /// the text the line must show, or `None` when the cursor did not move.
    ///
    /// `current` is the line as typed; it is saved when the cursor leaves the
    /// newest position.
    pub fn browse(&mut self, delta: isize, current: &str) -> Option<String> {
        let len = self.messages.len();
        let target = self.cursor.saturating_add_signed(delta).min(len);
        if target == self.cursor {
            return None;
        }
        if target == len {
            self.cursor = len;
            return Some(std::mem::take(&mut self.buffer));
        }
        if self.cursor == len {
            self.buffer = current.to_string();
        }
        self.cursor = target;
        self.messages.get(target).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(messages: &[&str]) -> SentHistory {
        let mut history = SentHistory::new();
        for message in messages {
            history.push(message);
        }
        history
    }

    #[test]
    fn up_walks_back_and_down_returns_to_the_unsent_line() {
        let mut history = history(&["first", "second"]);
        assert_eq!(history.browse(-1, "draft").as_deref(), Some("second"));
        assert_eq!(history.browse(-1, "second").as_deref(), Some("first"));
        assert_eq!(history.browse(-1, "first"), None, "the oldest is the limit");
        assert_eq!(history.browse(1, "first").as_deref(), Some("second"));
        assert_eq!(history.browse(1, "second").as_deref(), Some("draft"));
        assert_eq!(history.browse(1, "draft"), None);
    }

    #[test]
    fn a_repeat_of_the_last_message_is_not_added() {
        let history = history(&["a", "a", "b", "a"]);
        assert_eq!(history.messages(), ["a", "b", "a"]);
    }

    #[test]
    fn only_the_newest_hundred_messages_are_kept() {
        let mut history = history(&[]);
        for number in 0..150 {
            history.push(&number.to_string());
        }
        assert_eq!(history.messages().len(), MAX_HISTORY);
        assert_eq!(history.messages().first().map(String::as_str), Some("50"));
        assert_eq!(history.messages().last().map(String::as_str), Some("149"));
    }

    #[test]
    fn an_empty_history_never_moves() {
        let mut history = SentHistory::new();
        assert_eq!(history.browse(-1, "x"), None);
        assert_eq!(history.browse(1, "x"), None);
    }

    #[test]
    fn reopening_the_chat_starts_after_the_newest_message() {
        let mut history = history(&["a", "b"]);
        history.browse(-1, "");
        history.reset();
        assert_eq!(history.browse(-1, "").as_deref(), Some("b"));
    }
}
