// SPDX-License-Identifier: MIT OR Apache-2.0

//! The multiplayer server list: the saved book and the status pinger
//! (ROADMAP P10, ADR-0041).
//!
//! - [`book`] stores the named `host[:port]` entries in `servers.toml`.
//! - [`icon`] decodes the favicon a server answers with.
//! - [`pinger`] pings them in the background and reports MOTD, player counts,
//!   latency and favicon for the list screen to poll.

pub mod book;
pub mod icon;
pub mod pinger;

pub use book::{parse_address, ServerAddress, ServerBook, ServerEntry};
pub use icon::{decode_favicon, ICON_SIDE};
pub use pinger::{PingOutcome, PingResult, PingTarget, Pinger, ServerInfo};
pub use umicraft_ui::menus::ResourcePackMode;
