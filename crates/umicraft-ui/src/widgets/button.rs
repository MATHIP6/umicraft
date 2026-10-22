// SPDX-License-Identifier: MIT OR Apache-2.0

//! The push button widget.
//!
//! A button is stateless: it draws a filled rectangle whose colour depends on
//! the hover and press state reported by [`Ui::response`], centres a label in
//! the rectangle, and returns the same response to the caller. A press (not a
//! release) is the click, matching [`crate::UiResponse::clicked`]. The press
//! frame also draws the pressed colour, so the user sees the button react
//! before the action runs.
//!
//! The current response model has no disabled flag. A caller that must hide a
//! button draws nothing and skips the call, which also removes it from the
//! focus order; there is no half-disabled state to model.

use crate::context::{Ui, UiResponse};
use crate::draw::TextStyle;
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::text::FontAtlas;

use super::style;

/// Draws a push button and reports its interaction state.
///
/// `id` must stay stable across frames, `atlas` supplies the label metrics so
/// the label is centred with the font the renderer actually uses, and `label`
/// may contain several characters. The label is clipped to `rect`, so a long
/// label cannot bleed into the surrounding screen.
///
/// The button calls [`Ui::response`] exactly once, before reading its state,
/// which keeps the focus registration and the first-hit rule of overlapping
/// widgets intact.
///
/// # Example
///
/// ```
/// use umicraft_ui::widgets::button;
/// use umicraft_ui::{FontAtlas, Rect, Ui, UiInput};
///
/// let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
/// let mut ui = Ui::new(viewport);
/// ui.begin_frame(viewport, UiInput::default());
///
/// let id = ui.id("ok");
/// let response = button(
///     &mut ui,
///     id,
///     Rect::new(10.0, 10.0, 80.0, 28.0),
///     &FontAtlas::fallback(),
///     "OK",
/// );
/// assert!(!response.clicked);
/// ui.end_frame();
/// ```
pub fn button(ui: &mut Ui, id: WidgetId, rect: Rect, atlas: &FontAtlas, label: &str) -> UiResponse {
    let response = ui.response(id, rect);

    let fill = if response.clicked {
        style::BUTTON_PRESSED
    } else if response.hovered {
        style::BUTTON_HOVER
    } else {
        style::BUTTON
    };
    ui.draw().rect(rect, fill);

    if label.is_empty() || rect.is_empty() {
        return response;
    }

    let scale = style::TEXT_SCALE;
    let width = atlas.measure(label, scale);
    let line_height = atlas.line_height(scale);
    let position = [
        rect.x + (rect.width - width).max(0.0) * 0.5,
        rect.y + (rect.height - line_height).max(0.0) * 0.5,
    ];

    ui.push_clip(rect);
    ui.draw()
        .text(position, label, TextStyle::new(scale, style::TEXT));
    ui.pop_clip();

    response
}
