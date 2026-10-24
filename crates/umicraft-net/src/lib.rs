// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-net
//!
//! Async networking on top of [`umicraft_protocol`] (spec NET-1/2/6):
//! - [`ping`] — 1.8 server list ping with latency and MOTD parsing.
//! - [`login`] — offline-mode login ending in a [`login::LoginProfile`].
//! - [`play`] — the I/O-free play session and the one cancel-safe loop that
//!   drives it (ADR-0010).
//! - the `umicraft-headless` binary drives all three from the CLI.
//!
//! Connections are built out of [`crate::connection::Connection`], which owns
//! the framing/compression/encryption layers.

pub mod connection;
pub mod error;
pub mod login;
pub mod ping;
pub mod play;
pub mod status;

pub use connection::Connection;
pub use error::NetError;
