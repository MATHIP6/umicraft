// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integer range lists such as `0-3 5 10-12`, used by `heights`, `days`,
//! `metadata` and the tile lists.
//!
//! The grammar follows OptiFine's `ConnectedParser.parseRangeListInt`
//! (ADR-0088):
//! - tokens are separated by spaces or commas, and empty tokens are skipped;
//! - a token is `n` or `a-b`, where `a` and `b` may come in either order;
//! - every number is a non-negative integer;
//! - one bad token invalidates the whole list.
//!
//! A negative number cannot be written: `-5` splits on the dash.

use thiserror::Error;

/// Why a range list could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RangeError {
    /// A token is neither a non-negative integer nor `a-b`.
    #[error("invalid range {0:?}")]
    InvalidToken(String),
}

/// An inclusive range of integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// The smallest value in the range.
    pub min: u32,
    /// The largest value in the range.
    pub max: u32,
}

impl Range {
    /// The range from `a` to `b`, in either order.
    #[must_use]
    pub fn new(a: u32, b: u32) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Whether `value` lies in the range.
    #[must_use]
    pub fn contains(self, value: i64) -> bool {
        (i64::from(self.min)..=i64::from(self.max)).contains(&value)
    }
}

/// A list of inclusive ranges. A value matches when any range contains it.
///
/// # Example
///
/// ```
/// use umicraft_packext::RangeList;
///
/// let heights = RangeList::parse("0-63, 100")?;
/// assert!(heights.contains(40));
/// assert!(heights.contains(100));
/// assert!(!heights.contains(64));
/// # Ok::<(), umicraft_packext::RangeError>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RangeList {
    ranges: Vec<Range>,
}

impl RangeList {
    /// Parses a space- or comma-separated list of `n` and `a-b` tokens.
    ///
    /// # Errors
    ///
    /// [`RangeError::InvalidToken`] with the first token that is not a
    /// non-negative integer or a two-sided range.
    pub fn parse(text: &str) -> Result<Self, RangeError> {
        let ranges = text
            .split([' ', ','])
            .filter(|token| !token.is_empty())
            .map(parse_token)
            .collect::<Result<_, _>>()?;
        Ok(Self { ranges })
    }

    /// Whether any range contains `value`.
    #[must_use]
    pub fn contains(&self, value: i64) -> bool {
        self.ranges.iter().any(|range| range.contains(value))
    }

    /// The ranges, in file order.
    #[must_use]
    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    /// Whether the list has no range (an empty or blank value).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

fn parse_token(token: &str) -> Result<Range, RangeError> {
    let invalid = || RangeError::InvalidToken(token.to_string());
    if !token.contains('-') {
        return parse_number(token)
            .map(|n| Range::new(n, n))
            .ok_or_else(invalid);
    }
    // As OptiFine's tokenizer, empty parts between dashes are dropped, so
    // `3--5` is `3-5` and `-5` is a single part (invalid).
    let mut parts = token.split('-').filter(|part| !part.is_empty());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) => match (parse_number(a), parse_number(b)) {
            (Some(a), Some(b)) => Ok(Range::new(a, b)),
            _ => Err(invalid()),
        },
        _ => Err(invalid()),
    }
}

/// A non-negative integer as Java's `Integer.parseInt` of a trimmed token
/// reads it (a leading `+` is allowed). Values above `i32::MAX` are invalid.
fn parse_number(text: &str) -> Option<u32> {
    let value: i32 = text.trim().parse().ok()?;
    u32::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_values_and_ranges() {
        let list = RangeList::parse("1 3-5").expect("valid");
        assert_eq!(list.ranges(), &[Range::new(1, 1), Range::new(3, 5)]);
    }

    #[test]
    fn commas_and_repeated_separators_are_accepted() {
        let list = RangeList::parse(" 1,,2 ,  7-8 ").expect("valid");
        assert_eq!(list.ranges().len(), 3);
    }

    #[test]
    fn a_reversed_range_is_reordered() {
        assert_eq!(
            RangeList::parse("9-2").expect("valid").ranges(),
            &[Range::new(2, 9)]
        );
    }

    #[test]
    fn contains_is_inclusive_and_rejects_negatives() {
        let list = RangeList::parse("0-3").expect("valid");
        assert!(list.contains(0));
        assert!(list.contains(3));
        assert!(!list.contains(4));
        assert!(!list.contains(-1));
    }

    #[test]
    fn one_bad_token_invalidates_the_list() {
        for text in ["1 x", "-5", "1-2-3", "4-", "2147483648"] {
            assert_eq!(
                RangeList::parse(text),
                Err(RangeError::InvalidToken(
                    text.rsplit(' ').next().unwrap_or(text).to_string()
                )),
                "{text}"
            );
        }
    }

    #[test]
    fn a_double_dash_is_one_separator() {
        assert_eq!(
            RangeList::parse("3--5").expect("valid").ranges(),
            &[Range::new(3, 5)]
        );
    }

    #[test]
    fn a_blank_value_is_an_empty_list() {
        assert!(RangeList::parse("  ").expect("valid").is_empty());
    }
}
