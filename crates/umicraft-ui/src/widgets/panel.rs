// SPDX-License-Identifier: MIT OR Apache-2.0

//! The panel widget: a filled background with an explicit content rectangle.
//!
//! A panel carries no state and registers no response. It is the visual base
//! of a screen: the caller fills the panel, receives the inset content area,
//! and lays the widgets out inside that area. Keeping the inset explicit means
//! the caller never has to guess the padding used by a screen.

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::{Insets, Rect};

/// Padding between a panel edge and its content rectangle, in pixels.
///
/// [`panel`] subtracts this value from every side of the panel rectangle, so
/// the returned content rectangle is exactly `rect.deflate(Insets::all(
/// PANEL_INSET))`.
pub const PANEL_INSET: f32 = 12.0;

/// Draws a panel background and returns its inset content rectangle.
///
/// `rect` is the whole panel in physical pixels and `color` is its fill. The
/// returned rectangle is the area that callers must use for their children;
/// it is clamped to a non-negative size when the panel is smaller than twice
/// [`PANEL_INSET`]. The panel itself does not clip its children.
///
/// # Example
///
/// ```
/// use umicraft_ui::widgets::{panel, PANEL_INSET};
/// use umicraft_ui::{Rect, Ui, UiInput};
///
/// let viewport = Rect::new(0.0, 0.0, 200.0, 100.0);
/// let mut ui = Ui::new(viewport);
/// ui.begin_frame(viewport, UiInput::default());
///
/// let content = panel(&mut ui, Rect::new(10.0, 10.0, 100.0, 50.0), [0.0, 0.0, 0.0, 1.0]);
/// assert_eq!(content, Rect::new(10.0 + PANEL_INSET, 10.0 + PANEL_INSET, 100.0 - 2.0 * PANEL_INSET, 50.0 - 2.0 * PANEL_INSET));
/// ui.end_frame();
/// ```
pub fn panel(ui: &mut Ui, rect: Rect, color: Color) -> Rect {
    ui.draw().rect(rect, color);
    rect.deflate(Insets::all(PANEL_INSET))
}
