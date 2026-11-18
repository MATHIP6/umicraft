// SPDX-License-Identifier: MIT OR Apache-2.0

//! Third-party capes (ADR-0081): which services to ask, how to read their
//! answers, how to turn a downloaded image into RGBA frames, and a worker
//! thread that does all of it away from the game and render threads.
//!
//! ```text
//! CapeQuery ──▶ CapeResolver (thread "cape-fetch")
//!                 per provider: request ─▶ [JSON ─▶ image URL] ─▶ PNG ─▶ CapeImage
//!               ◀── CapeResult (first provider with a cape, or none)
//! ```
//!
//! Everything a service sends is untrusted: hosts are checked against an
//! allowlist ([`is_allowed_cape_url`]), bodies are size-limited, decoded
//! images are bounded, and every failure is a [`CapeError`] that only costs
//! the one provider it came from.

mod allowlist;
mod error;
mod image;
mod provider;
mod resolver;
mod response;
#[cfg(test)]
mod test_server;
#[cfg(test)]
mod test_support;

pub use allowlist::is_allowed_cape_url;
pub use error::CapeError;
pub use image::{decode_cape, CapeImage};
pub use provider::{CapeOwner, CapeProvider, CapeRequest, ResponseKind};
pub use resolver::{CapeQuery, CapeResolver, CapeResult, FoundCape};
pub use response::{CapeSource, ImageSource};
