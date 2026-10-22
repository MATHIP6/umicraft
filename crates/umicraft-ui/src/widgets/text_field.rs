// SPDX-License-Identifier: MIT OR Apache-2.0

//! The single-line text field widget (W3).
//!
//! Intended contract: `text_field(ui, id, rect, state) -> UiResponse`. The
//! widget edits `TextFieldState::text` through the focus and the input
//! events, keeps a caret index in bounds, and draws a visible caret when it
//! owns the focus. It supports insertion, Backspace, Delete, Home, End and the
//! left/right arrows; selection is optional for M4.
