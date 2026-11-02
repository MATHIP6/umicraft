// SPDX-License-Identifier: MIT OR Apache-2.0

//! The language table behind `translate` chat components.
//!
//! The table is built from the `.lang` files the first-run setup downloads
//! (`umicraft-assets`: asset index entry `minecraft/lang/en_US.lang`, loaded
//! by the client). [`LangTable::builtin`] is only the fallback used before
//! that file exists.
//!
//! Sources: MCP `Locale.loadLocaleData` (`key=value` lines, `#` comments,
//! split at the first `=`, `%d` / `%1$.2f` rewritten to `%s`) and
//! `ChatComponentTranslation.initializeFromFormat` (the placeholder pattern
//! `%(?:(\d+)\$)?([A-Za-z%]|$)`, `%%` is a percent sign, only `s` is a
//! supported conversion).

use std::collections::HashMap;

use thiserror::Error;

/// A translation format the chat cannot expand.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FormatError {
    /// The format holds a conversion other than `%s` and `%%`
    /// (`ChatComponentTranslationFormatException`).
    #[error("unsupported format {0:?}")]
    Unsupported(String),
}

/// One piece of an expanded translation format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Literal text, with `%%` already turned into `%`.
    Text(String),
    /// The zero-based index of the argument to insert.
    Argument(usize),
}

/// The translation table for `translate` components.
#[derive(Debug, Clone, Default)]
pub struct LangTable {
    entries: HashMap<String, String>,
}

impl LangTable {
    /// A table with the system messages the vanilla server sends most, so the
    /// chat is readable before the language file is installed.
    #[must_use]
    pub fn builtin() -> Self {
        const ENTRIES: [(&str, &str); 8] = [
            ("multiplayer.player.joined", "%s joined the game"),
            ("multiplayer.player.left", "%s left the game"),
            ("chat.type.text", "<%s> %s"),
            ("chat.type.announcement", "[%s] %s"),
            ("multiplayer.disconnect.generic", "Disconnected"),
            ("multiplayer.disconnect.kicked", "Kicked by an operator: %s"),
            (
                "commands.generic.notFound",
                "Unknown command. Type /help for help.",
            ),
            ("death.attack.player", "%s was slain by %s"),
        ];
        Self {
            entries: ENTRIES
                .iter()
                .map(|(key, text)| ((*key).to_string(), (*text).to_string()))
                .collect(),
        }
    }

    /// Parses the text of a `.lang` file (`Locale.loadLocaleData`).
    ///
    /// Blank lines and lines starting with `#` are skipped, a line is split
    /// at its first `=`, and numeric conversions (`%d`, `%1$.2f`) become `%s`
    /// because the chat only substitutes strings.
    #[must_use]
    pub fn from_lang(text: &str) -> Self {
        let entries = text
            .lines()
            .map(|line| line.trim_end_matches('\r'))
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), numeric_to_string(value)))
            .collect();
        Self { entries }
    }

    /// Adds the entries of `other`, replacing the ones with the same key.
    pub fn extend(&mut self, other: Self) {
        self.entries.extend(other.entries);
    }

    /// The text for a key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Rewrites `%d`, `%f` and their numbered / width forms to `%s`
/// (`Locale.pattern = %(\d+\$)?[\d\.]*[df]`).
fn numeric_to_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        let (number, after) = match tail[digits..].strip_prefix('$') {
            Some(after) if digits > 0 => (&tail[..=digits], after),
            _ => ("", tail),
        };
        let width = after
            .bytes()
            .take_while(|byte| byte.is_ascii_digit() || *byte == b'.')
            .count();
        match after.as_bytes().get(width) {
            Some(b'd' | b'f') => {
                out.push('%');
                out.push_str(number);
                out.push('s');
                rest = &after[width + 1..];
            }
            _ => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Expands a translation format into text and argument pieces.
///
/// Placeholders follow `%(?:(\d+)\$)?([A-Za-z%]|$)`: `%s` takes the next
/// argument, `%2$s` the second one, `%%` is a literal percent sign. Any other
/// conversion is an error.
///
/// # Errors
///
/// Returns [`FormatError::Unsupported`] for a conversion other than `s`.
pub fn expand_format(format: &str) -> Result<Vec<Piece>, FormatError> {
    let chars: Vec<char> = format.chars().collect();
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut next_argument = 0;
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '%' {
            literal.push(chars[at]);
            at += 1;
            continue;
        }
        let digits = chars[at + 1..]
            .iter()
            .take_while(|character| character.is_ascii_digit())
            .count();
        let numbered = digits > 0 && chars.get(at + 1 + digits) == Some(&'$');
        let conversion_at = at + 1 + if numbered { digits + 1 } else { 0 };
        match chars.get(conversion_at).copied() {
            Some('%') if !numbered => literal.push('%'),
            Some('s') => {
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                let index = if numbered {
                    let number: String = chars[at + 1..at + 1 + digits].iter().collect();
                    number.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
                } else {
                    next_argument += 1;
                    Some(next_argument - 1)
                };
                let index = index.ok_or_else(|| FormatError::Unsupported("%0$s".to_string()))?;
                pieces.push(Piece::Argument(index));
            }
            Some(other) if other.is_ascii_alphabetic() || other == '%' => {
                return Err(FormatError::Unsupported(format!("%{other}")));
            }
            // A lone `%` (end of text, or before something the pattern does
            // not match) stays literal.
            _ => {
                literal.push('%');
                at += 1;
                continue;
            }
        }
        at = conversion_at + 1;
    }
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    Ok(pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Piece {
        Piece::Text(value.to_string())
    }

    #[test]
    fn a_lang_file_is_parsed_into_entries() {
        let table = LangTable::from_lang("# comment\n\na.b=Hello %s\nc=d=e\r\n #x=y\n");
        assert_eq!(table.get("a.b"), Some("Hello %s"));
        assert_eq!(table.get("c"), Some("d=e"));
        assert_eq!(table.get("# comment"), None);
        assert_eq!(table.len(), 3, "a space before # makes it a key");
    }

    #[test]
    fn numeric_conversions_become_string_conversions() {
        let table = LangTable::from_lang("k=%d of %2$d, %.2f and %1$05.1f, 100%% and 5%\n");
        assert_eq!(
            table.get("k"),
            Some("%s of %2$s, %s and %1$s, 100%% and 5%")
        );
    }

    #[test]
    fn later_tables_override_earlier_entries() {
        let mut table = LangTable::builtin();
        table.extend(LangTable::from_lang("chat.type.text=%s says %s\n"));
        assert_eq!(table.get("chat.type.text"), Some("%s says %s"));
        assert!(table.get("multiplayer.player.joined").is_some());
    }

    #[test]
    fn sequential_and_numbered_placeholders_expand() {
        assert_eq!(
            expand_format("a %s b %s"),
            Ok(vec![
                text("a "),
                Piece::Argument(0),
                text(" b "),
                Piece::Argument(1)
            ])
        );
        assert_eq!(
            expand_format("%2$s then %1$s"),
            Ok(vec![Piece::Argument(1), text(" then "), Piece::Argument(0)])
        );
    }

    #[test]
    fn a_doubled_percent_is_one_percent_sign() {
        assert_eq!(expand_format("100%% sure"), Ok(vec![text("100% sure")]));
    }

    #[test]
    fn a_trailing_or_lone_percent_stays_literal() {
        assert_eq!(expand_format("50%"), Ok(vec![text("50%")]));
        assert_eq!(expand_format("5% off"), Ok(vec![text("5% off")]));
    }

    #[test]
    fn an_unsupported_conversion_is_an_error() {
        assert_eq!(
            expand_format("%x"),
            Err(FormatError::Unsupported("%x".to_string()))
        );
    }
}
