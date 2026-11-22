// SPDX-License-Identifier: MIT OR Apache-2.0

//! Splits an expression into tokens.

use super::ExprError;

/// One token and where it starts.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Token {
    pub(super) kind: Tok,
    pub(super) offset: usize,
}

/// The kinds of token.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok {
    Num(f32),
    /// A name. It may contain `.` and `:` (`head.rx`, `body:head.ry`).
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    AndAnd,
    OrOr,
    Bang,
    LParen,
    RParen,
    Comma,
}

/// Tokenizes `source`.
pub(super) fn lex(source: &str) -> Result<Vec<Token>, ExprError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = if c.is_ascii_digit() || (c == b'.' && next_is_digit(bytes, i)) {
            i = scan(bytes, i, |b| b.is_ascii_digit() || b == b'.');
            let text = &source[start..i];
            Tok::Num(
                text.parse()
                    .map_err(|_| ExprError::BadNumber(text.to_string()))?,
            )
        } else if c.is_ascii_alphabetic() || c == b'_' {
            i = scan(bytes, i, |b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':')
            });
            Tok::Ident(source[start..i].to_string())
        } else {
            let (kind, len) = operator(bytes, i).ok_or_else(|| ExprError::UnexpectedChar {
                offset: i,
                // Slicing at a char boundary: `i` always starts a character
                // because every earlier token ended on ASCII.
                found: source[i..].chars().next().unwrap_or('\u{fffd}'),
            })?;
            i += len;
            kind
        };
        tokens.push(Token {
            kind,
            offset: start,
        });
    }
    Ok(tokens)
}

fn next_is_digit(bytes: &[u8], i: usize) -> bool {
    bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
}

fn scan(bytes: &[u8], from: usize, keep: impl Fn(u8) -> bool) -> usize {
    let mut i = from;
    while i < bytes.len() && keep(bytes[i]) {
        i += 1;
    }
    i
}

/// The operator or punctuation at `i` and its length.
fn operator(bytes: &[u8], i: usize) -> Option<(Tok, usize)> {
    let two = bytes.get(i..i + 2);
    let pair = match two {
        Some(b"<=") => Some(Tok::Le),
        Some(b">=") => Some(Tok::Ge),
        Some(b"==") => Some(Tok::EqEq),
        Some(b"!=") => Some(Tok::Ne),
        Some(b"&&") => Some(Tok::AndAnd),
        Some(b"||") => Some(Tok::OrOr),
        _ => None,
    };
    if let Some(kind) = pair {
        return Some((kind, 2));
    }
    let kind = match bytes[i] {
        b'+' => Tok::Plus,
        b'-' => Tok::Minus,
        b'*' => Tok::Star,
        b'/' => Tok::Slash,
        b'%' => Tok::Percent,
        b'<' => Tok::Lt,
        b'>' => Tok::Gt,
        b'!' => Tok::Bang,
        b'(' => Tok::LParen,
        b')' => Tok::RParen,
        b',' => Tok::Comma,
        _ => return None,
    };
    Some((kind, 1))
}
