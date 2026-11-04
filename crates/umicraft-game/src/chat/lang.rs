// SPDX-License-Identifier: MIT OR Apache-2.0

//! The language table behind `translate` chat components.
//!
//! The table itself lives in [`umicraft_core::lang`] since the UI crates need
//! it too (ADR-0039); this module keeps the historical
//! `umicraft_game::chat::lang` path working.

pub use umicraft_core::lang::{expand_format, FormatError, LangTable, Piece};
