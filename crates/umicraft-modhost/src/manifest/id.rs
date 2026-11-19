// SPDX-License-Identifier: MIT OR Apache-2.0

//! Validated identifier newtypes.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use crate::error::IdError;

/// Longest identifier, in characters, so ids stay usable as file and table names.
const MAX_LEN: usize = 64;

/// Checks `^[a-z][a-z0-9_<extra>]{0,max_len-1}$` without a regex dependency.
///
/// Shared by manifest ids and by the keys and names a mod declares at run time
/// (settings, widgets, key bindings, commands), so one grammar rule has one
/// implementation.
pub(crate) fn is_valid(text: &str, extra: &[char], max_len: usize) -> bool {
    let mut chars = text.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    first_ok
        && text.len() <= max_len
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || extra.contains(&c))
}

/// Defines an identifier newtype with its grammar, parsing and display.
macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident, $kind:literal, $grammar:literal, $extra:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
        #[serde(try_from = "String")]
        pub struct $name(String);

        impl $name {
            /// The identifier as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(value: String) -> Result<Self, IdError> {
                if is_valid(&value, $extra, MAX_LEN) {
                    Ok(Self(value))
                } else {
                    Err(IdError { kind: $kind, value, grammar: $grammar })
                }
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<Self, IdError> {
                Self::try_from(text.to_owned())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identifier!(
    /// The id of a mod: `^[a-z][a-z0-9_]{0,63}$`.
    ModId,
    "mod id",
    "^[a-z][a-z0-9_]{0,63}$",
    &[]
);

identifier!(
    /// The id of a replaceable or wrappable host service: `^[a-z][a-z0-9_.]{0,63}$`.
    ServiceId,
    "service id",
    "^[a-z][a-z0-9_.]{0,63}$",
    &['.']
);
