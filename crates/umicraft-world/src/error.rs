// SPDX-License-Identifier: MIT OR Apache-2.0

//! World errors.

use thiserror::Error;

/// Errors produced while applying server world data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorldError {
    /// A chunk payload's byte length does not match the length derived from
    /// its section mask, dimension, and `ground_up` flag.
    #[error(
        "chunk payload has {actual} bytes, expected {expected} \
         (mask {primary_bit_mask:#06x}, sky_light {sky_light}, ground_up {ground_up})"
    )]
    ChunkPayloadLength {
        /// Length the wire format requires.
        expected: usize,
        /// Length the server actually sent.
        actual: usize,
        /// Section mask from the packet header.
        primary_bit_mask: u16,
        /// Whether sky-light nibbles are present (overworld).
        sky_light: bool,
        /// Whether the payload ends with the 256-byte biome array.
        ground_up: bool,
    },
}
