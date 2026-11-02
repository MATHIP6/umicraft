// SPDX-License-Identifier: MIT OR Apache-2.0

//! The state of a Tab completion between two key presses.
//!
//! Sources: MCP `GuiChat.autocompletePlayerNames` (cycling through the
//! answer, listing it when there are several matches),
//! `GuiChat.onAutocompleteResponse` (a longer common prefix is written
//! first; an answer is accepted only while `waitingOnAutocomplete`) and
//! commons-lang `StringUtils.getCommonPrefix`.

/// The completion state of the chat input.
#[derive(Debug, Clone, Default)]
pub struct Completion {
    /// A request is in flight; only then is a response accepted
    /// (`waitingOnAutocomplete`).
    pub waiting: bool,
    /// The matches Tab cycles through (`foundPlayerNames`).
    pub matches: Vec<String>,
    /// The next match to write (`autocompleteIndex`).
    pub index: usize,
    /// Tab is cycling through `matches` instead of asking the server
    /// (`playerNamesFound`).
    pub cycling: bool,
}

impl Completion {
    /// Any key other than Tab ends the completion (`keyTyped` clears the
    /// flags before it looks at the key).
    pub fn interrupt(&mut self) {
        self.waiting = false;
        self.cycling = false;
    }

    /// Forgets the matches of an earlier request.
    pub fn clear_matches(&mut self) {
        self.matches.clear();
        self.index = 0;
    }

    /// The next match to write, wrapping to the first, and the list to print
    /// when there are several. Advances the cursor.
    pub fn next_match(&mut self) -> (Option<String>, Option<String>) {
        if self.index >= self.matches.len() {
            self.index = 0;
        }
        let listing = (self.matches.len() > 1).then(|| self.matches.join(", "));
        let name = self.matches.get(self.index).cloned();
        self.index += 1;
        (name, listing)
    }
}

/// The longest prefix shared by every string (`StringUtils.getCommonPrefix`).
#[must_use]
pub fn common_prefix(strings: &[String]) -> String {
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

    #[test]
    fn the_common_prefix_is_character_based() {
        assert_eq!(common_prefix(&names(&["abc", "abd"])), "ab");
        assert_eq!(common_prefix(&names(&["abc"])), "abc");
        assert_eq!(common_prefix(&names(&["abc", "xyz"])), "");
        assert_eq!(common_prefix(&[]), "");
        assert_eq!(common_prefix(&names(&["\u{e9}a", "\u{e9}b"])), "\u{e9}");
    }

    #[test]
    fn next_match_wraps_and_lists_only_several_matches() {
        let mut completion = Completion {
            matches: names(&["Steve", "Stella"]),
            ..Completion::default()
        };
        let (first, listing) = completion.next_match();
        assert_eq!(first.as_deref(), Some("Steve"));
        assert_eq!(listing.as_deref(), Some("Steve, Stella"));
        assert_eq!(completion.next_match().0.as_deref(), Some("Stella"));
        assert_eq!(completion.next_match().0.as_deref(), Some("Steve"));

        let mut single = Completion {
            matches: names(&["Alex"]),
            ..Completion::default()
        };
        assert_eq!(single.next_match(), (Some("Alex".into()), None));
    }
}
