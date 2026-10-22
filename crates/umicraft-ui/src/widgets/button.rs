// SPDX-License-Identifier: MIT OR Apache-2.0

//! The push button widget (W3).
//!
//! Intended contract: `button(ui, id, rect, label) -> UiResponse`. The button
//! draws a filled rectangle and a centred label with the current font atlas,
//! reports hover/press through [`crate::UiResponse`], and never keeps state
//! between frames.
