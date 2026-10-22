// SPDX-License-Identifier: MIT OR Apache-2.0

//! The M4 acceptance screen: a button, a text field and a scrollable list.
//!
//! W3 owns this module. The screen must be a pure function of its state and of
//! [`crate::UiInput`], so the windowed client, the screenshot path and the
//! headless driver all exercise exactly the same code. The client only feeds
//! input and renders the resulting [`crate::DrawList`].
//!
//! The screen is opened with F2 in the windowed client and with `--ui-demo` in
//! the headless/offscreen path; the acceptance criteria are listed in the M4
//! ADR.
