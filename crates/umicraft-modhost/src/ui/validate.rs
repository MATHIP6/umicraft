// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checks shared by every kind of declaration.

use crate::error::DeclareError;
use crate::manifest::is_identifier;

/// Grammar of setting, widget and key-binding keys.
pub(crate) const KEY_GRAMMAR: &str = "^[a-z][a-z0-9_]{0,63}$";
/// Grammar of command names.
pub(crate) const COMMAND_GRAMMAR: &str = "^[a-z][a-z0-9_-]{0,31}$";

/// Checks a setting, widget or key-binding key.
pub(crate) fn check_key(what: &'static str, key: &str) -> Result<(), DeclareError> {
    check_identifier(what, key, &[], 64, KEY_GRAMMAR)
}

/// Checks a command name (`-` is allowed, unlike in keys).
pub(crate) fn check_command_name(name: &str) -> Result<(), DeclareError> {
    check_identifier("command name", name, &['-'], 32, COMMAND_GRAMMAR)
}

fn check_identifier(
    what: &'static str,
    value: &str,
    extra: &[char],
    max: usize,
    grammar: &'static str,
) -> Result<(), DeclareError> {
    if is_identifier(value, extra, max) {
        Ok(())
    } else {
        Err(DeclareError::InvalidKey {
            what,
            value: value.to_owned(),
            grammar,
        })
    }
}

/// Checks a text of at most `max` bytes; it may be empty.
pub(crate) fn check_len(what: &'static str, text: &str, max: usize) -> Result<(), DeclareError> {
    if text.len() <= max {
        Ok(())
    } else {
        Err(DeclareError::Text { what, max })
    }
}

/// Checks a text of 1 to `max` bytes.
pub(crate) fn check_text(what: &'static str, text: &str, max: usize) -> Result<(), DeclareError> {
    if text.is_empty() {
        return Err(DeclareError::Text { what, max });
    }
    check_len(what, text, max)
}

/// Checks that the list of `what` has room for one more of at most `max`.
pub(crate) fn check_room(what: &'static str, len: usize, max: usize) -> Result<(), DeclareError> {
    if len < max {
        Ok(())
    } else {
        Err(DeclareError::TooMany { what, max })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_command_names_follow_their_grammar() {
        assert!(check_key("key", "show_fps").is_ok());
        assert!(check_key("key", "Show").is_err());
        assert!(check_key("key", "a-b").is_err());
        assert!(check_key("key", &"a".repeat(65)).is_err());
        assert!(check_command_name("my-cmd").is_ok());
        assert!(check_command_name("1cmd").is_err());
        assert!(check_command_name(&"a".repeat(33)).is_err());
    }

    #[test]
    fn texts_are_bounded_in_bytes() {
        assert!(check_text("label", "", 4).is_err());
        assert!(check_text("label", "abcd", 4).is_ok());
        assert!(check_text("label", "abcde", 4).is_err());
        assert!(check_len("label", "", 4).is_ok());
        assert!(check_text("label", "\u{30c4}\u{30c4}", 4).is_err());
    }
}
