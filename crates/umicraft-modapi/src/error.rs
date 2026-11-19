// SPDX-License-Identifier: MIT OR Apache-2.0

//! The error type of mod-side calls.

use std::fmt;

use umicraft_modapi_sys::UmiStatus;

/// Why a mod call failed.
///
/// Hand-written `Display`/`Error` because this crate depends on nothing but
/// the ABI types, so mod binaries stay small.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModError {
    /// The host answered a request with a non-OK status.
    HostRefused(UmiStatus),
    /// A mod-defined failure, typically from [`Mod::init`](crate::Mod::init).
    Message(String),
}

impl ModError {
    /// Builds a [`ModError::Message`] from anything string-like.
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

impl ModError {
    /// Maps a host status to `Ok` for `OK` and to a refusal otherwise.
    pub(crate) fn check(status: UmiStatus) -> Result<(), Self> {
        if status.is_ok() {
            Ok(())
        } else {
            Err(Self::HostRefused(status))
        }
    }
}

impl fmt::Display for ModError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostRefused(status) => {
                write!(f, "host refused the request (status {})", status.0)
            }
            Self::Message(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for ModError {}
