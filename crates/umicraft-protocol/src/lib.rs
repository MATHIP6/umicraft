// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-protocol
//!
//! Minecraft Java Edition 1.8 (protocol version 47) wire protocol: VarInt
//! coding, packet framing with zlib compression, encryption helpers (AES-128 /
//! CFB8, RSA, the session-server hash), and packet definitions for the
//! handshake / status / login / play states.
//!
//! All parsing is written for hostile input: malformed, truncated, or oversized
//! wire data produces typed errors ([`error::ProtocolError`]) and never panics
//! (spec NET-13). Behavior sources are cited per module and collected in
//! `umicraft/docs/sources.md`.

pub mod encryption;
pub mod error;
pub mod framing;
pub mod packets;
pub mod state;
pub mod types;
pub mod varint;

pub use error::ProtocolError;
pub use framing::Framing;
pub use packets::{
    Handshake, LoginClientbound, LoginServerbound, PlayClientbound, PlayServerbound,
    StatusClientbound, StatusServerbound,
};
pub use state::{State, PROTOCOL_VERSION_1_8};
