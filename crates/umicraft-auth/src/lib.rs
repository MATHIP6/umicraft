// SPDX-License-Identifier: MIT OR Apache-2.0
//! Player session handling for Umicraft.
//!
//! As the vanilla 1.8.8 client, Umicraft receives a session (username, UUID,
//! access token) from the launcher through `--username`, `--uuid` and
//! `--accessToken` (see ADR-0053). This crate parses it ([`launch`]), keeps
//! the token out of logs ([`Secret`]), and performs the session-server `join`
//! call needed by the online-mode handshake ([`join`]).
//!
//! No session means an offline session; Microsoft login belongs to F1.

pub mod error;
pub mod join;
pub mod launch;
pub mod secret;
pub mod session;

pub use error::AuthError;
pub use join::{HttpSessionServer, SessionServer};
pub use launch::{session_from_launch, strip_session_args};
pub use secret::Secret;
pub use session::Session;
