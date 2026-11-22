// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `.properties` files of the pack features.
//!
//! OptiFine reads them with `java.util.Properties.load(InputStream)`, so this
//! parser follows that format:
//! - bytes are ISO-8859-1, so every byte is one character and decoding cannot
//!   fail;
//! - a line whose first non-blank character is `#` or `!` is a comment;
//! - a key ends at the first unescaped `=`, `:` or blank, and one separator
//!   plus the blanks around it are skipped;
//! - a line ending in an odd number of `\` continues on the next line, whose
//!   leading blanks are dropped;
//! - the escapes are `\t`, `\n`, `\r`, `\f`, `\uXXXX`, and `\x` for any other
//!   `x`;
//! - a later duplicate key replaces an earlier one.
//!
//! Source: the `java.util.Properties.load` contract (Java SE API
//! documentation).

use std::collections::BTreeMap;

use thiserror::Error;

/// Why a `.properties` file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PropertiesError {
    /// A `\u` escape is not followed by four hexadecimal digits. Java rejects
    /// the whole file in that case, and so do we.
    #[error("malformed \\uXXXX escape on line {line}")]
    MalformedUnicodeEscape {
        /// The 1-based physical line the escape starts on.
        line: usize,
    },
}

/// The key/value pairs of one `.properties` file.
///
/// # Example
///
/// ```
/// use umicraft_packext::Properties;
///
/// let props = Properties::parse(b"# a sky layer\nstartFadeIn=20:00\nblend = add\n")?;
/// assert_eq!(props.get("startFadeIn"), Some("20:00"));
/// assert_eq!(props.get("blend"), Some("add"));
/// assert_eq!(props.get("speed"), None);
/// # Ok::<(), umicraft_packext::PropertiesError>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    entries: BTreeMap<String, String>,
}

impl Properties {
    /// Parses `bytes` as an ISO-8859-1 `.properties` file.
    ///
    /// # Errors
    ///
    /// [`PropertiesError::MalformedUnicodeEscape`] when a `\u` escape is
    /// incomplete or not hexadecimal.
    pub fn parse(bytes: &[u8]) -> Result<Self, PropertiesError> {
        let mut entries = BTreeMap::new();
        for line in logical_lines(bytes) {
            let (key, value) = split_entry(&line.text);
            entries.insert(unescape(key, line.number)?, unescape(value, line.number)?);
        }
        Ok(Self { entries })
    }

    /// The value of `key` with surrounding blanks trimmed, as every OptiFine
    /// reader trims it. `None` when the key is absent.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|value| value.trim())
    }

    /// Every key, sorted. Features with numbered keys (`skins.1`, `tile.3`)
    /// scan them with this.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the file has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One logical line: continuations joined, escapes not yet processed.
struct LogicalLine {
    /// The 1-based physical line it starts on.
    number: usize,
    text: Vec<char>,
}

/// The blanks of the format: space, tab and form feed.
fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{c}')
}

/// Splits `bytes` into the logical lines that carry an entry, dropping blank
/// and comment lines.
fn logical_lines(bytes: &[u8]) -> Vec<LogicalLine> {
    let physical = split_physical(bytes);
    let mut lines = Vec::new();
    let mut index = 0;
    while index < physical.len() {
        let number = index + 1;
        let first = trim_start(&physical[index]);
        index += 1;
        if first.is_empty() || matches!(first[0], '#' | '!') {
            continue;
        }
        let mut text = first.to_vec();
        while ends_with_continuation(&text) && index < physical.len() {
            text.pop();
            text.extend_from_slice(trim_start(&physical[index]));
            index += 1;
        }
        if ends_with_continuation(&text) {
            // A continuation on the last line continues into nothing.
            text.pop();
        }
        lines.push(LogicalLine { number, text });
    }
    lines
}

/// Splits on `\n`, `\r` and `\r\n`, decoding each byte as ISO-8859-1.
fn split_physical(bytes: &[u8]) -> Vec<Vec<char>> {
    let mut lines = vec![Vec::new()];
    let mut previous_cr = false;
    for &byte in bytes {
        match byte {
            b'\n' if previous_cr => {}
            b'\n' | b'\r' => lines.push(Vec::new()),
            _ => {
                if let Some(line) = lines.last_mut() {
                    line.push(char::from(byte));
                }
            }
        }
        previous_cr = byte == b'\r';
    }
    lines
}

fn trim_start(line: &[char]) -> &[char] {
    let start = line
        .iter()
        .position(|&c| !is_blank(c))
        .unwrap_or(line.len());
    &line[start..]
}

/// Whether the line ends in an odd run of backslashes.
fn ends_with_continuation(text: &[char]) -> bool {
    text.iter().rev().take_while(|&&c| c == '\\').count() % 2 == 1
}

/// Splits a logical line into its raw key and raw value.
fn split_entry(text: &[char]) -> (&[char], &[char]) {
    let mut key_end = text.len();
    let mut escaped = false;
    for (index, &c) in text.iter().enumerate() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '=' || c == ':' || is_blank(c) {
            key_end = index;
            break;
        }
    }
    let mut rest = trim_start(&text[key_end..]);
    if let Some((&first, tail)) = rest.split_first() {
        if first == '=' || first == ':' {
            rest = trim_start(tail);
        }
    }
    (&text[..key_end], rest)
}

/// Processes the escapes of a raw key or value.
fn unescape(raw: &[char], line: usize) -> Result<String, PropertiesError> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.iter().copied();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(escaped) = chars.next() else {
            break;
        };
        out.push(match escaped {
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            'f' => '\u{c}',
            'u' => unicode_escape(&mut chars, line)?,
            other => other,
        });
    }
    Ok(out)
}

/// Reads the four hex digits of a `\u` escape.
fn unicode_escape(
    chars: &mut impl Iterator<Item = char>,
    line: usize,
) -> Result<char, PropertiesError> {
    let malformed = PropertiesError::MalformedUnicodeEscape { line };
    let mut code = 0u32;
    for _ in 0..4 {
        let digit = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| malformed.clone())?;
        code = code * 16 + digit;
    }
    // A lone surrogate is not a `char`; Java would keep it, we replace it.
    Ok(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Properties {
        Properties::parse(text.as_bytes()).expect("valid properties")
    }

    #[test]
    fn the_three_separators_are_accepted() {
        let props = parse("a=1\nb:2\nc 3\nd = 4\ne\t:\t5\n");
        let values: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|key| props.get(key))
            .collect();
        assert_eq!(
            values,
            [Some("1"), Some("2"), Some("3"), Some("4"), Some("5")]
        );
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let props = parse("# comment\n! also\n\n   \nkey=value\n");
        assert_eq!(props.len(), 1);
        assert_eq!(props.get("key"), Some("value"));
    }

    #[test]
    fn a_key_alone_has_an_empty_value() {
        assert_eq!(parse("flag\n").get("flag"), Some(""));
    }

    #[test]
    fn continuation_lines_are_joined_without_their_indent() {
        let props = parse("tiles=0-3 \\\n      4 5\nnext=1");
        assert_eq!(props.get("tiles"), Some("0-3 4 5"));
        assert_eq!(props.get("next"), Some("1"));
    }

    #[test]
    fn an_even_run_of_backslashes_does_not_continue() {
        let props = parse("path=a\\\\\nnext=1");
        assert_eq!(props.get("path"), Some("a\\"));
        assert_eq!(props.get("next"), Some("1"));
    }

    #[test]
    fn escapes_are_processed_in_keys_and_values() {
        let props = parse("a\\=b=c\\td\nu=\\u0041\\u00e9\n");
        assert_eq!(props.get("a=b"), Some("c\td"));
        assert_eq!(props.get("u"), Some("Aé"));
    }

    #[test]
    fn a_malformed_unicode_escape_is_an_error_with_its_line() {
        assert_eq!(
            Properties::parse(b"ok=1\nbad=\\u12g4\n"),
            Err(PropertiesError::MalformedUnicodeEscape { line: 2 })
        );
        assert_eq!(
            Properties::parse(b"bad=\\u12"),
            Err(PropertiesError::MalformedUnicodeEscape { line: 1 })
        );
    }

    #[test]
    fn bytes_are_latin_1() {
        let props = Properties::parse(b"name=caf\xe9\n").expect("latin-1 never fails");
        assert_eq!(props.get("name"), Some("café"));
    }

    #[test]
    fn every_line_ending_is_accepted() {
        let props = parse("a=1\r\nb=2\rc=3\n");
        assert_eq!(props.len(), 3);
        assert_eq!(props.get("b"), Some("2"));
    }

    #[test]
    fn the_last_duplicate_wins_and_values_are_trimmed() {
        let props = parse("blend=add\nblend=  alpha  \n");
        assert_eq!(props.get("blend"), Some("alpha"));
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..512)) {
            let _ = Properties::parse(&bytes);
        }
    }
}
