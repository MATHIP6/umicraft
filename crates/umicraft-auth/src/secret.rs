// SPDX-License-Identifier: MIT OR Apache-2.0
//! A string wrapper that cannot be printed by accident.

use std::fmt;

/// A sensitive string (such as an access token).
///
/// It deliberately has no `Display` and a redacted `Debug`, so logging a
/// struct that contains it cannot leak the value. Use [`Secret::expose`] at
/// the single place where the value is really needed.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps `value`.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret value; callers must not log it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_contains_value() {
        let s = Secret::new("hunter2-token");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert!(!format!("{s:#?}").contains("hunter2"));
    }

    #[test]
    fn expose_returns_value() {
        assert_eq!(Secret::new("abc").expose(), "abc");
    }
}
