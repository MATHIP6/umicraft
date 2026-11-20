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
    /// The host's bounded queue is full (`UmiStatus::BUSY`): the request is
    /// valid and may succeed if retried later. Only
    /// [`Packets::send`](crate::Packets::send) can fail this way.
    Busy,
    /// The chunk asked about is not loaded (`UmiStatus::UNAVAILABLE`). Only
    /// [`World::block`](crate::World::block) can fail this way; the answer may
    /// change once the chunk arrives.
    Unavailable,
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
    /// Maps a host status to `Ok` for `OK`, to [`ModError::Busy`] for `BUSY`,
    /// to [`ModError::Unavailable`] for `UNAVAILABLE` and to a refusal
    /// otherwise.
    pub(crate) fn check(status: UmiStatus) -> Result<(), Self> {
        if status.is_ok() {
            Ok(())
        } else if status == UmiStatus::BUSY {
            Err(Self::Busy)
        } else if status == UmiStatus::UNAVAILABLE {
            Err(Self::Unavailable)
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
            Self::Busy => f.write_str("the host is busy, try again later"),
            Self::Unavailable => f.write_str("the chunk is not loaded"),
            Self::Message(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for ModError {}
