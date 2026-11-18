// SPDX-License-Identifier: MIT OR Apache-2.0

//! The typed failures of a cape lookup.
//!
//! Every byte a cape service sends is untrusted (spec §14.3), so each way a
//! response can be wrong has its own variant. The resolver only logs them: a
//! provider that fails is the same as a provider without a cape.

/// Why a cape could not be fetched or decoded.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CapeError {
    /// The request failed (DNS, TLS, timeout, truncated body, ...).
    #[error("cape request failed: {0}")]
    Http(String),

    /// A response body was larger than the limit for its kind.
    #[error("cape response is larger than {limit} bytes")]
    TooLarge {
        /// The limit that was exceeded, in bytes.
        limit: usize,
    },

    /// The bytes are not a PNG the decoder accepts, or a data URL is broken.
    #[error("cape image cannot be decoded: {0}")]
    Decode(String),

    /// The image has a size no cape can have (zero, over 2048, a strip that
    /// is not made of `w x w/2` frames, or more than 64 frames).
    #[error("cape image size {width}x{height} is not acceptable")]
    BadSize {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },

    /// A JSON answer is not valid JSON.
    #[error("cape answer is not valid json: {0}")]
    BadJson(String),

    /// A URL in an answer points outside the allowed hosts.
    #[error("cape url `{0}` is not on an allowed host")]
    HostNotAllowed(String),
}
