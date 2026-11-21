// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ordered lists of non-negative integers such as `1 3-5 9`, where a range is
//! expanded and the order is kept (`weights.1`, `skins.1`, `damage` masks).
//!
//! The rules follow OptiFine's `ConnectedParser.parseIntList` (decompiled
//! 1.8.9 build, ADR-0088): tokens are split on space and comma, `a-b` expands
//! to every number from `a` to `b`, and an invalid token is dropped with a
//! warning instead of invalidating the list. Unlike [`RangeList`](crate::RangeList),
//! this keeps duplicates and order, which a weighted pick needs.

/// The most numbers one list may hold. A range such as `0-2000000000` would
/// otherwise allocate gigabytes from a hostile pack.
pub const MAX_LEN: usize = 65_536;

/// Parses `text`, dropping invalid tokens with a warning. The list stops
/// growing at [`MAX_LEN`] entries.
///
/// # Example
///
/// ```
/// use umicraft_packext::int_list::parse;
///
/// assert_eq!(parse("1 3-5,9"), [1, 3, 4, 5, 9]);
/// assert_eq!(parse("2 x 4"), [2, 4]);
/// ```
#[must_use]
pub fn parse(text: &str) -> Vec<u32> {
    let mut list = Vec::new();
    for token in text.split([' ', ',']).filter(|token| !token.is_empty()) {
        match parse_token(token) {
            Some((first, last)) => {
                let room = MAX_LEN.saturating_sub(list.len());
                list.extend((first..=last).take(room));
            }
            None => tracing::warn!(token, "invalid number in a list, ignored"),
        }
    }
    list
}

/// `n` as `(n, n)`, `a-b` as `(a, b)` with `a <= b`.
fn parse_token(token: &str) -> Option<(u32, u32)> {
    let number = |text: &str| text.parse::<i32>().ok().and_then(|n| u32::try_from(n).ok());
    match token.split_once('-') {
        None => number(token).map(|n| (n, n)),
        Some((first, last)) => {
            let (first, last) = (number(first)?, number(last)?);
            (first <= last).then_some((first, last))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_ranges_keep_their_order_and_duplicates() {
        assert_eq!(parse("5 1-3 5"), [5, 1, 2, 3, 5]);
    }

    #[test]
    fn invalid_tokens_are_dropped_one_by_one() {
        assert_eq!(parse("1 -2 3-1 a 7 4-"), [1, 7]);
    }

    #[test]
    fn a_huge_range_is_capped() {
        assert_eq!(parse("0-2000000000").len(), MAX_LEN);
        assert_eq!(parse("0-70000 5").len(), MAX_LEN);
    }

    #[test]
    fn an_empty_text_is_an_empty_list() {
        assert!(parse("  ").is_empty());
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_text_never_panics(text in ".{0,60}") {
            let _ = parse(&text);
        }
    }
}
