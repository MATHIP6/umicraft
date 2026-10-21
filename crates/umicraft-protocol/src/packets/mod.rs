// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packet definitions and codecs for the M1 protocol subset, organized by
//! state and direction:
//!
//! - `handshake` — [`Handshake`] (state selector)
//! - [`status`] — server-list request/response and ping/pong
//! - [`login`] — login start/success, encryption request/response, disconnect,
//!   set compression
//! - [`play`] — the minimal join + keep-alive subset
//!
//! Direction convention: `clientbound` packets flow server → client (inbound
//! on a client); `serverbound` packets flow client → server (outbound on a
//! client). The names follow the wiki.vg tables and the state enums.
//!
//! Every packet is a standalone struct owning its wire layout (`ID`,
//! `decode`, [`EncodablePacket`], and tests); the state enums are dispatch
//! tables over those structs. Encoding contract: `encode_body` writes the
//! id's *body* only — the id itself travels via [`packet_id`], and the outer
//! length/compression layers live in [`crate::framing`].
//!
//! [`packet_id`]: EncodablePacket::packet_id

use std::io::Write;

use crate::error::ProtocolError;

pub mod codec;
pub mod handshake;
pub mod login;
pub mod play;
pub mod status;

pub use codec::{
    read_bool, read_byte_array, read_f32, read_f64, read_i32, read_i64, read_rest, read_u8,
};
pub use handshake::Handshake;
pub use login::{LoginClientbound, LoginServerbound};
pub use play::{PlayClientbound, PlayServerbound};
pub use status::{StatusClientbound, StatusServerbound};

/// Unified write side shared by packet types across states and directions.
///
/// Framing layers ([`crate::Framing`]) and the connection write path only need
/// these two pieces plus a fresh body buffer per packet.
pub trait EncodablePacket {
    /// Wire packet id for the packet's own direction and state.
    fn packet_id(&self) -> u8;

    /// Writes the body only; the caller writes the id via [`Self::packet_id`]
    /// and wraps the bytes in the outer length/compression layers.
    ///
    /// # Errors
    /// [`ProtocolError`] on oversized fields or stream errors.
    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError>;
}
