// SPDX-License-Identifier: MIT OR Apache-2.0

//! The M4 widgets: button, text field, scrollable list and panel.
//!
//! The module files are created by the M4 setup so that the workstreams never
//! edit the same `mod` declaration. W3 owns the implementations and the unit
//! tests; the intended public functions are frozen in
//! `docs/decisions/0006-m4-ui-foundation.md` (names and semantics, not exact
//! parameter types).
//!
//! A widget draws through [`crate::Ui::draw`] and reports its state with
//! [`crate::Ui::response`]. Widgets own no renderer resources and never touch
//! the window or the network.

pub mod button;
pub mod list;
pub mod panel;
pub mod text_field;
