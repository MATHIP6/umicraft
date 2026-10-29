// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat input line: editing, the history of sent messages and the Tab
//! completion (M10, GAM-6).
//!
//! The behaviour follows MCP `GuiChat` (`keyTyped`, `getSentHistory`,
//! `autocompletePlayerNames`, `onAutocompleteResponse`) and `GuiTextField`
//! (`writeText`, `func_146197_a` word movement, `deleteFromCursor`). The type
//! is I/O-free: the window feeds it keys and sends the requests it returns.

/// The longest message the input accepts (`setMaxStringLength(100)`).
pub const MAX_LEN: usize = 100;
/// The sent messages kept for the up and down keys.
pub const MAX_HISTORY: usize = 100;

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

/// The completion state between two Tab presses.
#[derive(Debug, Clone, Default)]
struct Completion {
    /// A request is in flight; only then is a response accepted.
    waiting: bool,
    /// The matches Tab cycles through.
    matches: Vec<String>,
    /// The next match to write.
    index: usize,
    /// Tab is cycling through `matches` instead of asking the server.
    cycling: bool,
}

/// The chat input line.
#[derive(Debug, Clone)]
pub struct ChatInput {
    text: Vec<char>,
    caret: usize,
    /// Where the up and down keys are in the sent history.
    history_cursor: usize,
    /// The line being typed, kept while the history is browsed.
    history_buffer: String,
    completion: Completion,
}

/// Adds a sent message to the history: consecutive duplicates collapse and
/// the oldest message goes first (`GuiNewChat.addToSentMessages`).
pub fn push_sent(sent: &mut Vec<String>, message: &str) {
    if sent.last().map(String::as_str) != Some(message) {
        sent.push(message.to_string());
    }
    if sent.len() > MAX_HISTORY {
        sent.remove(0);
    }
}

impl ChatInput {
    /// A line holding `initial` (a leading `/` when the command key opened
    /// the chat), with the history cursor after the `sent_len` messages.
    #[must_use]
    pub fn new(initial: &str, sent_len: usize) -> Self {
        let mut input = Self {
            text: Vec::new(),
            caret: 0,
            history_cursor: sent_len,
            history_buffer: String::new(),
            completion: Completion::default(),
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
        self.caret
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
        self.completion.waiting = false;
        self.completion.cycling = false;
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
            self.text.insert(self.caret, character);
            self.caret += 1;
        }
    }

    /// Types text at the caret.
    pub fn insert(&mut self, text: &str) {
        self.interrupt();
        self.write(text);
    }

    /// Replaces the whole line and puts the caret at its end.
    pub fn set_text(&mut self, text: &str) {
        self.interrupt();
        self.text.clear();
        self.caret = 0;
        self.write(text);
    }

    /// The caret position `delta` words away (`func_146197_a`): a word move
    /// skips the spaces next to the caret, then the word.
    fn word_boundary(&self, backwards: bool, from: usize, skip_spaces: bool) -> usize {
        let mut position = from;
        if backwards {
            while skip_spaces && position > 0 && self.text[position - 1] == ' ' {
                position -= 1;
            }
            while position > 0 && self.text[position - 1] != ' ' {
                position -= 1;
            }
        } else {
            while position < self.text.len() && self.text[position] != ' ' {
                position += 1;
            }
            while skip_spaces && position < self.text.len() && self.text[position] == ' ' {
                position += 1;
            }
        }
        position
    }

    /// Deletes the character before the caret, or the word before it.
    pub fn backspace(&mut self, word: bool) {
        self.interrupt();
        if self.caret == 0 {
            return;
        }
        let start = if word {
            self.word_boundary(true, self.caret, true)
        } else {
            self.caret - 1
        };
        self.text.drain(start..self.caret);
        self.caret = start;
    }

    /// Deletes the character after the caret, or the word after it.
    pub fn delete(&mut self, word: bool) {
        self.interrupt();
        if self.caret >= self.text.len() {
            return;
        }
        let end = if word {
            self.word_boundary(false, self.caret, true)
        } else {
            self.caret + 1
        };
        self.text.drain(self.caret..end);
    }

    /// Moves the caret one character or one word left.
    pub fn left(&mut self, word: bool) {
        self.interrupt();
        self.caret = if word {
            self.word_boundary(true, self.caret, true)
        } else {
            self.caret.saturating_sub(1)
        };
    }

    /// Moves the caret one character or one word right.
    pub fn right(&mut self, word: bool) {
        self.interrupt();
        self.caret = if word {
            self.word_boundary(false, self.caret, true)
        } else {
            (self.caret + 1).min(self.text.len())
        };
    }

    /// Moves the caret to the start of the line.
    pub fn home(&mut self) {
        self.interrupt();
        self.caret = 0;
    }

    /// Moves the caret to the end of the line.
    pub fn end(&mut self) {
        self.interrupt();
        self.caret = self.text.len();
    }

    /// Shows the previous sent message (the up key).
    pub fn history_previous(&mut self, sent: &[String]) {
        self.browse_history(-1, sent);
    }

    /// Shows the next sent message, then the line being typed (the down key).
    pub fn history_next(&mut self, sent: &[String]) {
        self.browse_history(1, sent);
    }

    fn browse_history(&mut self, delta: isize, sent: &[String]) {
        self.interrupt();
        let len = sent.len();
        let target = (self.history_cursor as isize + delta).clamp(0, len as isize) as usize;
        if target == self.history_cursor {
            return;
        }
        if target == len {
            self.history_cursor = len;
            let buffer = std::mem::take(&mut self.history_buffer);
            self.set_text(&buffer);
        } else {
            if self.history_cursor == len {
                self.history_buffer = self.text();
            }
            self.history_cursor = target;
            self.set_text(&sent[target]);
        }
    }

    /// The start of the word before the caret (`func_146197_a(-1, …, false)`).
    fn word_start(&self) -> usize {
        self.word_boundary(true, self.caret, false)
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
        self.completion.matches.clear();
        self.completion.index = 0;
        if self.caret == 0 {
            return TabOutcome::Nothing;
        }
        self.completion.waiting = true;
        TabOutcome::Request(self.text[..self.caret].iter().collect())
    }

    /// Replaces the word before the caret with the next match and returns the
    /// list to print when there are several.
    fn write_next_match(&mut self) -> Option<String> {
        let start = self.word_start();
        self.text.drain(start..self.caret);
        self.caret = start;
        if self.completion.index >= self.completion.matches.len() {
            self.completion.index = 0;
        }
        let listing =
            (self.completion.matches.len() > 1).then(|| self.completion.matches.join(", "));
        if let Some(name) = self.completion.matches.get(self.completion.index).cloned() {
            self.write(&name);
        }
        self.completion.index += 1;
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
        let word: String = self.text[self.word_start()..self.caret].iter().collect();
        let prefix = common_prefix(matches);
        if !prefix.is_empty() && !word.eq_ignore_ascii_case(&prefix) {
            let start = self.word_start();
            self.text.drain(start..self.caret);
            self.caret = start;
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

/// The longest prefix shared by every string (`StringUtils.getCommonPrefix`).
fn common_prefix(strings: &[String]) -> String {
    let Some(first) = strings.first() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for other in &strings[1..] {
        let shared = prefix
            .iter()
            .zip(other.chars())
            .take_while(|(a, b)| **a == *b)
            .count();
        prefix.truncate(shared);
    }
    prefix.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn typed(text: &str) -> ChatInput {
        let mut input = ChatInput::new("", 0);
        input.insert(text);
        input
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut input = typed("helo");
        input.left(false);
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
        input.home();
        input.delete(false);
        assert_eq!(input.text(), "ne two ");
        input.delete(true);
        assert_eq!(input.text(), "two ");
        // At the edges nothing happens and nothing panics.
        input.home();
        input.backspace(false);
        input.end();
        input.delete(false);
        assert_eq!(input.text(), "two ");
    }

    #[test]
    fn the_caret_moves_by_characters_words_and_to_the_edges() {
        let mut input = typed("ab cd ef");
        input.left(true);
        assert_eq!(input.caret(), 6);
        input.left(true);
        assert_eq!(input.caret(), 3);
        input.right(true);
        assert_eq!(input.caret(), 6);
        input.home();
        input.left(false);
        assert_eq!(input.caret(), 0);
        input.end();
        input.right(false);
        assert_eq!(input.caret(), 8);
    }

    #[test]
    fn the_history_walks_back_and_returns_to_the_unsent_line() {
        let sent = names(&["first", "second"]);
        let mut input = ChatInput::new("", sent.len());
        input.insert("draft");
        input.history_previous(&sent);
        assert_eq!(input.text(), "second");
        input.history_previous(&sent);
        assert_eq!(input.text(), "first");
        input.history_previous(&sent);
        assert_eq!(input.text(), "first", "the oldest message is the limit");
        input.history_next(&sent);
        assert_eq!(input.text(), "second");
        input.history_next(&sent);
        assert_eq!(input.text(), "draft", "down returns to the typed line");
        input.history_next(&sent);
        assert_eq!(input.text(), "draft");
    }

    #[test]
    fn sent_messages_collapse_duplicates_and_keep_one_hundred() {
        let mut sent = Vec::new();
        push_sent(&mut sent, "a");
        push_sent(&mut sent, "a");
        push_sent(&mut sent, "b");
        push_sent(&mut sent, "a");
        assert_eq!(sent, ["a", "b", "a"]);
        for number in 0..150 {
            push_sent(&mut sent, &number.to_string());
        }
        assert_eq!(sent.len(), MAX_HISTORY);
        assert_eq!(sent.last().unwrap(), "149");
    }

    #[test]
    fn tab_on_an_empty_line_asks_for_nothing() {
        let mut input = ChatInput::new("", 0);
        assert_eq!(input.tab(), TabOutcome::Nothing);
    }

    #[test]
    fn tab_asks_the_server_with_the_text_before_the_caret() {
        let mut input = typed("/tp St");
        assert_eq!(input.tab(), TabOutcome::Request("/tp St".to_string()));
        input.left(false);
        input.left(false);
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
    fn the_common_prefix_is_character_based() {
        assert_eq!(common_prefix(&names(&["abc", "abd"])), "ab");
        assert_eq!(common_prefix(&names(&["abc"])), "abc");
        assert_eq!(common_prefix(&names(&["abc", "xyz"])), "");
        assert_eq!(common_prefix(&[]), "");
        assert_eq!(common_prefix(&names(&["\u{e9}a", "\u{e9}b"])), "\u{e9}");
    }

    #[test]
    fn a_completion_respects_the_length_limit() {
        let mut input = typed(&format!("{} a", "b".repeat(96)));
        input.tab();
        input.on_completions(&names(&["a-very-long-name"]));
        assert!(input.len() <= MAX_LEN);
    }
}
