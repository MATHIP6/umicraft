// SPDX-License-Identifier: MIT OR Apache-2.0

//! The string condition of pack features: `name`, `nbt.<path>` and the like.
//!
//! The syntax is the one OptiFine's `NbtTagValue` reads (decompiled 1.8.9
//! build, ADR-0088) and that the public OptiFineDoc describes for
//! `cit.properties`, `random_entities.properties` and
//! `custom_guis.properties`:
//! - plain text matches the whole string;
//! - `pattern:` and `ipattern:` use the wildcards `*` (any run) and `?` (one
//!   character), the second ignoring case;
//! - `regex:` and `iregex:` use a regular expression that must match the
//!   whole string, the second ignoring case;
//! - a leading `!` negates the condition.
//!
//! Regular expressions use the Rust `regex` syntax, which is the common
//! subset of Java's for the patterns packs write. OptiFine also runs the
//! value through Java's string unescaping; this reader leaves that to
//! [`Properties`](crate::Properties), as the documentation's examples imply.

use regex::{Regex, RegexBuilder};
use thiserror::Error;

/// Why a [`TextMatch`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TextMatchError {
    /// The `regex:` or `iregex:` expression does not compile.
    #[error("invalid regular expression: {0}")]
    BadRegex(String),
}

/// How the text is compared.
#[derive(Debug, Clone)]
enum Matcher {
    /// Whole-string equality.
    Text(String),
    /// A compiled expression anchored at both ends. Covers the wildcards too.
    Expression(Regex),
}

/// A string condition, see the [module documentation](self).
///
/// # Example
///
/// ```
/// use umicraft_packext::TextMatch;
///
/// let name = TextMatch::parse("ipattern:pink*").expect("valid");
/// assert!(name.matches("Pinky"));
/// assert!(!name.matches("Blinky"));
/// assert!(TextMatch::parse("!Pinky").expect("valid").matches("Blinky"));
/// ```
#[derive(Debug, Clone)]
pub struct TextMatch {
    matcher: Matcher,
    negated: bool,
}

impl TextMatch {
    /// Parses the value of a condition.
    ///
    /// # Errors
    ///
    /// [`TextMatchError::BadRegex`] when a regular expression is invalid.
    pub fn parse(value: &str) -> Result<Self, TextMatchError> {
        let (negated, value) = match value.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, value),
        };
        let matcher = if let Some(rest) = value.strip_prefix("pattern:") {
            expression(&wildcards_to_regex(rest), false)?
        } else if let Some(rest) = value.strip_prefix("ipattern:") {
            expression(&wildcards_to_regex(rest), true)?
        } else if let Some(rest) = value.strip_prefix("regex:") {
            expression(rest, false)?
        } else if let Some(rest) = value.strip_prefix("iregex:") {
            expression(rest, true)?
        } else {
            Matcher::Text(value.to_string())
        };
        Ok(Self { matcher, negated })
    }

    /// Whether `text` satisfies the condition, negation included.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        let found = match &self.matcher {
            Matcher::Text(expected) => expected == text,
            Matcher::Expression(regex) => regex.is_match(text),
        };
        found != self.negated
    }

    /// Whether the condition holds for an absent value: only a negated one
    /// does (`name=!Pinky` matches an unnamed entity).
    #[must_use]
    pub fn matches_absent(&self) -> bool {
        self.negated
    }
}

/// Compiles `source` anchored at both ends, so the whole text must match.
fn expression(source: &str, ignore_case: bool) -> Result<Matcher, TextMatchError> {
    RegexBuilder::new(&format!("^(?:{source})$"))
        .case_insensitive(ignore_case)
        .size_limit(1 << 20)
        .build()
        .map(Matcher::Expression)
        .map_err(|error| TextMatchError::BadRegex(error.to_string()))
}

/// The regular expression of a wildcard pattern: `*` and `?` stand for
/// themselves as wildcards, every other character is literal.
fn wildcards_to_regex(pattern: &str) -> String {
    let mut source = String::with_capacity(pattern.len() * 2);
    for c in pattern.chars() {
        match c {
            '*' => source.push_str(".*"),
            '?' => source.push('.'),
            other => source.push_str(&regex::escape(other.encode_utf8(&mut [0; 4]))),
        }
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> TextMatch {
        TextMatch::parse(value).expect("valid")
    }

    #[test]
    fn plain_text_matches_the_whole_string() {
        assert!(text("Pinky").matches("Pinky"));
        assert!(!text("Pinky").matches("pinky"));
        assert!(!text("Pinky").matches("Pinky2"));
    }

    #[test]
    fn patterns_use_star_and_question_mark() {
        let pattern = text("pattern:Pin?y*");
        assert!(pattern.matches("Pinky the pig"));
        assert!(!pattern.matches("Pinky".to_lowercase().as_str()));
        assert!(!text("pattern:a.b").matches("axb"), "a dot is literal");
    }

    #[test]
    fn ipattern_and_iregex_ignore_case() {
        assert!(text("ipattern:PINK*").matches("pinky"));
        assert!(text("iregex:pin(k)+y").matches("PINKKY"));
        assert!(!text("regex:pin(k)+y").matches("PINKKY"));
    }

    #[test]
    fn regex_must_match_the_whole_text() {
        assert!(text("regex:Pin(k)+y").matches("Pinkky"));
        assert!(!text("regex:Pin").matches("Pinky"));
        assert!(text(r"regex:\d+").matches("123"));
    }

    #[test]
    fn a_bang_negates_and_an_absent_value_only_matches_a_negation() {
        assert!(text("!Pinky").matches("Blinky"));
        assert!(!text("!Pinky").matches("Pinky"));
        assert!(text("!Pinky").matches_absent());
        assert!(!text("Pinky").matches_absent());
    }

    #[test]
    fn a_bad_expression_is_an_error() {
        assert!(matches!(
            TextMatch::parse("regex:(unclosed"),
            Err(TextMatchError::BadRegex(_))
        ));
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_values_never_panic(value in ".{0,40}", probe in ".{0,20}") {
            if let Ok(condition) = TextMatch::parse(&value) {
                let _ = condition.matches(&probe);
            }
        }
    }
}
