// SPDX-License-Identifier: MIT OR Apache-2.0

//! The launcher session of the command line (ADR-0053).
//!
//! A launcher starts the vanilla client with `--username`, `--uuid` and
//! `--accessToken`. They are read before [`crate::args::Args`] parses the
//! rest, so the token never reaches the other parsers nor the usage text.

use umicraft_auth::{session_from_launch, strip_session_args, Session};

/// Splits the launcher session off `words`.
///
/// Returns the words without the session flags, and the session when the
/// flags or the `UMICRAFT_*` environment variables carry one.
///
/// # Errors
/// A message naming the missing or invalid flags (never their values) when
/// the session is incomplete.
pub fn split(words: &[String]) -> Result<(Vec<String>, Option<Session>), String> {
    let session = session_from_launch(words, |name| std::env::var(name).ok())
        .map_err(|error| error.to_string())?;
    Ok((strip_session_args(words), session))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_session_flags_are_split_off_before_the_other_parsers() {
        let (rest, session) = split(&words(&[
            "host",
            "--username",
            "Pixel",
            "--uuid",
            "069a79f4-44e9-4726-a5be-fca90e38aaf5",
            "--accessToken=secret-token",
            "--view-distance",
            "6",
        ]))
        .expect("complete session");
        assert_eq!(rest, words(&["host", "--view-distance", "6"]));
        let session = session.expect("a session");
        assert_eq!(session.username, "Pixel");
        assert!(!format!("{session:?}").contains("secret-token"));
    }

    #[test]
    fn a_partial_session_is_an_error_that_hides_the_values() {
        let error = split(&words(&["--username", "Pixel", "--accessToken", "tok"]))
            .expect_err("uuid is missing");
        assert!(error.contains("uuid"), "{error}");
        assert!(!error.contains("tok"), "{error}");
    }
}
