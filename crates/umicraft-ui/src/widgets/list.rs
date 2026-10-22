// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scrollable list widget (W3).
//!
//! Intended contract: `list(ui, id, rect, items, state) -> UiResponse`. The
//! list clips its rows to its viewport, scrolls with the wheel and the arrow
//! keys, and reports selection changes through [`crate::UiResponse::changed`].
