// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `nbt.<path>=<value>` condition of a rule.
//!
//! OptiFine's `NbtTagValue` walks a compound by the dotted path and compares
//! the tag it finds as a string. This crate has no NBT reader, so the caller
//! gives a lookup that returns the values at a path as strings (a number is
//! its decimal text); a `*` segment, which stands for every child of a
//! compound or list, may return several. The condition holds when any of them
//! matches. A leading `!` negates the whole condition, so an absent tag
//! satisfies it.

use crate::{TextMatch, TextMatchError};

/// Looks up the values at a path of segments, as strings.
pub type NbtLookup<'a> = &'a dyn Fn(&[&str]) -> Vec<String>;

/// How the value is compared.
#[derive(Debug, Clone)]
enum Value {
    Text(TextMatch),
    /// A `#rrggbb` colour, compared with an integer tag.
    Color(i64),
}

/// One `nbt.` condition.
#[derive(Debug, Clone)]
pub struct NbtCondition {
    path: Vec<String>,
    value: Value,
    negated: bool,
}

impl NbtCondition {
    /// Builds the condition of `nbt.<path>=<value>`; `path` is the part after
    /// `nbt.`.
    ///
    /// # Errors
    ///
    /// [`TextMatchError`] when the value is an invalid regular expression.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_packext::cit::NbtCondition;
    ///
    /// let name = NbtCondition::parse("display.Name", "ipattern:my *").expect("valid");
    /// let tag = |path: &[&str]| match path {
    ///     ["display", "Name"] => vec!["My Sword".to_string()],
    ///     _ => Vec::new(),
    /// };
    /// assert!(name.matches(&tag));
    /// ```
    pub fn parse(path: &str, value: &str) -> Result<Self, TextMatchError> {
        let (negated, value) = match value.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, value),
        };
        Ok(Self {
            path: path
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect(),
            value: parse_value(value)?,
            negated,
        })
    }

    /// Whether the tags behind `lookup` satisfy the condition.
    #[must_use]
    pub fn matches(&self, lookup: NbtLookup<'_>) -> bool {
        let path: Vec<&str> = self.path.iter().map(String::as_str).collect();
        let found = lookup(&path).iter().any(|text| match &self.value {
            Value::Text(condition) => condition.matches(text),
            Value::Color(color) => text.parse::<i64>().is_ok_and(|n| n == *color),
        });
        found != self.negated
    }
}

fn parse_value(value: &str) -> Result<Value, TextMatchError> {
    if let Some(hex) = value.strip_prefix('#') {
        let is_color =
            hex.len() == 6 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if let (true, Ok(color)) = (is_color, i64::from_str_radix(hex, 16)) {
            return Ok(Value::Color(color));
        }
    }
    TextMatch::parse(value).map(Value::Text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(
        values: &'static [(&'static str, &'static [&'static str])],
    ) -> impl Fn(&[&str]) -> Vec<String> {
        move |path: &[&str]| {
            let key = path.join(".");
            values
                .iter()
                .filter(|(name, _)| *name == key)
                .flat_map(|(_, found)| found.iter().map(|text| (*text).to_string()))
                .collect()
        }
    }

    #[test]
    fn a_value_is_compared_as_text_or_pattern() {
        let tags = lookup(&[("display.Name", &["Excalibur"])]);
        assert!(NbtCondition::parse("display.Name", "Excalibur")
            .expect("ok")
            .matches(&tags));
        assert!(NbtCondition::parse("display.Name", "pattern:Exc*")
            .expect("ok")
            .matches(&tags));
        assert!(!NbtCondition::parse("display.Name", "excalibur")
            .expect("ok")
            .matches(&tags));
    }

    #[test]
    fn a_wildcard_path_matches_any_of_its_values() {
        let tags = lookup(&[("display.Lore.*", &["first", "second"])]);
        assert!(NbtCondition::parse("display.Lore.*", "second")
            .expect("ok")
            .matches(&tags));
        assert!(!NbtCondition::parse("display.Lore.*", "third")
            .expect("ok")
            .matches(&tags));
    }

    #[test]
    fn a_negation_holds_for_an_absent_tag() {
        let tags = lookup(&[("display.Name", &["Excalibur"])]);
        let not = NbtCondition::parse("display.Name", "!Excalibur").expect("ok");
        assert!(!not.matches(&tags));
        assert!(not.matches(&lookup(&[])));
        assert!(!NbtCondition::parse("display.Name", "Excalibur")
            .expect("ok")
            .matches(&lookup(&[])));
    }

    #[test]
    fn a_hex_colour_matches_an_integer_tag() {
        let tags = lookup(&[("display.color", &["16751052"])]);
        assert!(NbtCondition::parse("display.color", "#ff99cc")
            .expect("ok")
            .matches(&tags));
        assert!(!NbtCondition::parse("display.color", "#ff99cd")
            .expect("ok")
            .matches(&tags));
    }

    #[test]
    fn a_bad_regular_expression_is_an_error() {
        assert!(NbtCondition::parse("display.Name", "regex:(").is_err());
    }
}
