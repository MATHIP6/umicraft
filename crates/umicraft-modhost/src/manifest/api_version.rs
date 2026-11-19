//! The `major.minor` API version a mod declares.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

/// A malformed `major.minor` text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("`{0}` is not an API version, expected `major.minor` such as `0.1`")]
pub struct ApiVersionError(String);

/// The modding API version, `major.minor`.
///
/// Mirrors the compatibility rules of ADR-0087, on the manifest side: the C type
/// lives in `umicraft-modapi-sys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct ApiVersion {
    /// Breaking changes bump this.
    pub major: u16,
    /// Additive changes bump this.
    pub minor: u16,
}

impl ApiVersion {
    /// Whether a host of version `self` can run a mod built for `required`.
    ///
    /// Before 1.0 every minor may break, so major 0 needs an exact match; from 1.0 on
    /// the major must match and the host may be newer in minor (ADR-0087).
    pub fn accepts(self, required: ApiVersion) -> bool {
        if self.major == 0 {
            self == required
        } else {
            self.major == required.major && self.minor >= required.minor
        }
    }
}

/// Parses one decimal component; `u16::from_str` alone would accept `+1`.
fn component(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

impl FromStr for ApiVersion {
    type Err = ApiVersionError;

    fn from_str(text: &str) -> Result<Self, ApiVersionError> {
        text.split_once('.')
            .and_then(|(major, minor)| Some((component(major)?, component(minor)?)))
            .map(|(major, minor)| Self { major, minor })
            .ok_or_else(|| ApiVersionError(text.to_owned()))
    }
}

impl TryFrom<String> for ApiVersion {
    type Error = ApiVersionError;

    fn try_from(text: String) -> Result<Self, ApiVersionError> {
        text.parse()
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}
