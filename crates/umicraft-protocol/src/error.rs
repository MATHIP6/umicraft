// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed errors for `umicraft-protocol`.
//!
//! Every decode path is fallible by design (spec NET-13): malformed or
//! oversized input from a server becomes an error, never a panic.

/// Error type for protocol framing, decoding, and encryption helpers.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// A VarInt exceeded the protocol maximum of 5 bytes for 32-bit values.
    #[error("varint longer than the 5-byte protocol maximum")]
    VarIntTooLong,

    /// A packet (or its inflated size) exceeded the 2 MiB protocol limit.
    #[error("packet size {size} bytes exceeds the protocol limit of {limit}")]
    PacketTooLarge {
        /// Claimed size in bytes.
        size: usize,
        /// The protocol maximum that was exceeded.
        limit: usize,
    },

    /// A string exceeded its declared character budget.
    #[error("string of {chars} chars exceeds the {limit} char limit")]
    StringTooLong {
        /// Declared length from the wire.
        chars: usize,
        /// Maximum allowed.
        limit: usize,
    },

    /// Wire bytes were meant to be text but are not UTF-8.
    #[error("string is not valid UTF-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    /// Underlying I/O failed (short reads, socket errors).
    #[error("io error")]
    Io(#[from] std::io::Error),

    /// Zlib inflate/deflate failed; the stream bitstream is damaged.
    #[error("zlib stream error")]
    Zlib(#[from] flate2::DecompressError),

    /// A packet id was not recognized for its protocol state.
    #[error("unknown packet id {id:#04x} for {state:?} state")]
    UnknownPacket {
        /// Raw packet id from the wire.
        id: u8,
        /// Connection state the packet arrived in.
        state: crate::state::State,
    },

    /// A packet arrived with fields that do not satisfy its definition
    /// (lengths, enums, invariants) beyond simple size limits.
    #[error("packet field invariant violated: {0}")]
    Invariant(String),
}
