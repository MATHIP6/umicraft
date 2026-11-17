// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat of the session: the history, the visibility the server was told
//! and the completions it offered.

use std::collections::VecDeque;

use umicraft_render::ChatLineView;

/// The number of chat messages kept (MCP GuiNewChat keeps 100). The HUD
/// wraps them and shows the newest rows that fit.
pub(in crate::game) const CHAT_HISTORY: usize = 100;

/// The chat history, its visibility and the tab completions.
#[derive(Default)]
pub(in crate::game) struct ChatState {
    /// The chat messages, oldest first.
    pub(in crate::game) lines: VecDeque<ChatLineView>,
    /// The chat visibility the server was told (`ClientSettings` byte).
    pub(in crate::game) mode: u8,
    /// The completions the server offered for the open chat line.
    pub(in crate::game) completions: Vec<String>,
    /// Counts the tab-completion answers.
    pub(in crate::game) completions_serial: u64,
}

impl ChatState {
    /// Adds a message, dropping the oldest beyond [`CHAT_HISTORY`].
    pub(in crate::game) fn push(&mut self, line: ChatLineView) {
        self.lines.push_back(line);
        while self.lines.len() > CHAT_HISTORY {
            self.lines.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn line(text: &str) -> ChatLineView {
        crate::chat::line_from_runs(&umicraft_game::chat::local(text), Instant::now())
    }

    #[test]
    fn the_history_drops_the_oldest_message_beyond_its_limit() {
        let mut chat = ChatState::default();
        for index in 0..=CHAT_HISTORY {
            chat.push(line(&index.to_string()));
        }
        assert_eq!(chat.lines.len(), CHAT_HISTORY);
        let kept: Vec<String> = chat.lines.iter().map(ChatLineView::plain).collect();
        assert_eq!(kept.first().map(String::as_str), Some("1"));
        assert_eq!(kept.last(), Some(&CHAT_HISTORY.to_string()));
    }
}
