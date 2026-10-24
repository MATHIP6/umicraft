// SPDX-License-Identifier: MIT OR Apache-2.0

//! Play state after login (ADR-0010).
//!
//! - [`PlaySession`] turns each clientbound packet into a [`Reaction`]
//!   without performing I/O.

mod session;

pub use session::{JoinGameInfo, PlayEvent, PlayOptions, PlaySession, PlayerPlacement, Reaction};
