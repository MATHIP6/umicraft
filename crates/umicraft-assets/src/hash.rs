// SPDX-License-Identifier: MIT OR Apache-2.0

//! SHA-1 helpers used to verify every byte that comes from the network.
//!
//! The official asset endpoints publish the SHA-1 of every file they serve
//! (the `sha1` field of the asset index and of the version JSON), so the
//! transport layer re-computes the digest of what it received and compares it
//! before anything is written to disk (spec AST-1/AST-3).
//!
//! The digests are rendered as lowercase hex, which is the form Mojang uses in
//! the manifest, the version JSON, and the asset index.

use sha1::{Digest, Sha1};

use crate::error::{AssetsError, AssetsResult};

/// Renders bytes as a lowercase hex string.
///
/// # Examples
///
/// ```
/// assert_eq!(umicraft_assets::hash::to_hex(&[0x0f, 0xa0]), "0fa0");
/// ```
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Computes the SHA-1 digest of `bytes` and renders it as lowercase hex.
///
/// # Examples
///
/// ```
/// // The SHA-1 of the empty input is a well-known constant.
/// assert_eq!(umicraft_assets::hash::sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
/// ```
#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

/// Verifies that `bytes` hash to `expected` (a lowercase hex SHA-1).
///
/// # Errors
///
/// [`AssetsError::HashMismatch`] when the digests differ.
///
/// # Examples
///
/// ```
/// use umicraft_assets::hash::verify_sha1;
///
/// let bytes = b"hello";
/// let digest = umicraft_assets::hash::sha1_hex(bytes);
/// assert!(verify_sha1(bytes, &digest).is_ok());
/// assert!(verify_sha1(bytes, "0000000000000000000000000000000000000000").is_err());
/// ```
pub fn verify_sha1(bytes: &[u8], expected: &str) -> AssetsResult<()> {
    let actual = sha1_hex(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(AssetsError::HashMismatch {
            what: expected.to_string(),
            expected: expected.to_string(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_renders_every_byte() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xf0, 0xff]), "000ff0ff");
        assert_eq!(to_hex(&[]), "");
    }

    #[test]
    fn sha1_of_the_empty_input_matches_the_published_constant() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn sha1_matches_the_published_vector_for_abc() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn verify_accepts_the_matching_digest_and_rejects_another_one() {
        let bytes = b"abc";
        let digest = sha1_hex(bytes);
        assert!(verify_sha1(bytes, &digest).is_ok());

        let err = verify_sha1(bytes, "da39a3ee5e6b4b0d3255bfef95601890afd80709")
            .expect_err("a wrong digest must be rejected");
        assert!(matches!(err, AssetsError::HashMismatch { .. }));
    }
}
