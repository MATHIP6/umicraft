// SPDX-License-Identifier: MIT OR Apache-2.0

//! Play state after login (ADR-0010).
//!
//! - [`PlaySession`] turns each clientbound packet into a [`Reaction`]
//!   without performing I/O.
//! - [`drive`] is the async loop that feeds the session from the socket and
//!   writes its replies and the caller's commands.

mod drive;
mod session;

pub use drive::{drive, SessionCounters, SessionEnd};

pub use session::{JoinGameInfo, PlayEvent, PlayOptions, PlaySession, PlayerPlacement, Reaction};
