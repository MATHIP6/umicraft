// SPDX-License-Identifier: MIT OR Apache-2.0

//! The multiplayer server list: the saved book and the status pinger
//! (ROADMAP P10, ADR-0041).
//!
//! - [`book`] stores the named `host[:port]` entries in `servers.toml`.
//! - [`pinger`] pings them in the background and reports MOTD, player counts,
//!   latency and favicon for the list screen to poll.

pub mod book;
pub mod pinger;

pub use book::{
    parse_address, AddressError, BookError, EditError, ServerAddress, ServerBook, ServerEntry,
    DEFAULT_NAME, DEFAULT_PORT,
};
pub use pinger::{
    PingOutcome, PingResult, PingTarget, Pinger, PingerStartError, ServerInfo, MAX_FAVICON_BYTES,
    PING_TIMEOUT,
};
