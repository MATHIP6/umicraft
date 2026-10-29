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
    button_enabled(ui, id, rect, atlas, label, true)
}

/// Draws a push button with an explicit enabled state.
///
/// When `enabled` is false, clicks and hover states are ignored, and the button
/// is rendered with the disabled texture from `widgets.png` (or dim background)
/// and dimmed text color.
pub fn button_enabled(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    label: &str,
    enabled: bool,
) -> UiResponse {
    button_tinted(ui, id, rect, atlas, label, enabled, None)
}

/// Like [`button_enabled`], with the label in `tint` (vanilla colours a
/// conflicting key red and a key waiting for input yellow). The tint only
/// applies while the button is enabled.
pub fn button_tinted(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    label: &str,
    enabled: bool,
    tint: Option<[f32; 4]>,
) -> UiResponse {
    let mut response = ui.response(id, rect);
    if !enabled {
        response.clicked = false;
        response.hovered = false;
    }

    if let Some(sheet) = ui.widgets_sheet() {
        let v_y = if !enabled {
            46
        } else if response.clicked || response.hovered {
            86
        } else {
            66
        };
        // The texture is cut in texels (GUI units), then scaled, as vanilla does.
        let skin = (rect.height / 20.0).round().max(1.0);
        let texels = (rect.width / skin).round().min(400.0);
        let half = (texels / 2.0).floor();
        let other = texels - half;
        let src_left = sheet.sub(0, v_y, half as u32, 20);
        let src_right = sheet.sub(200 - other as u32, v_y, other as u32, 20);
        let dst_left = Rect::new(rect.x, rect.y, half * skin, rect.height);
        let dst_right = Rect::new(
            rect.x + half * skin,
            rect.y,
            rect.width - half * skin,
            rect.height,
        );
        ui.draw().image(src_left, dst_left, [1.0; 4]);
        ui.draw().image(src_right, dst_right, [1.0; 4]);
    } else {
        let fill = if !enabled {
            style::BUTTON_DISABLED
        } else if response.clicked {
            style::BUTTON_PRESSED
        } else if response.hovered {
            style::BUTTON_HOVER
        } else {
            style::BUTTON
        };
        ui.draw().rect(rect, fill);
    }

    if label.is_empty() || rect.is_empty() {
        return response;
    }

    let pad = 6.0;
    let available = (rect.width - pad).max(0.0);
    let natural_scale = if ui.widgets_sheet().is_some() {
        (rect.height / 20.0).round().max(1.0)
    } else {
        style::TEXT_SCALE
    };
    let mut scale = natural_scale;
    let mut width = atlas.measure(label, scale);
    if width > available && available > 0.0 {
        scale = (scale * available / width).max(1.0);
        width = atlas.measure(label, scale);
    }
    let glyph_h = 8.0 * scale;
    let position = [
        (rect.x + (rect.width - width).max(0.0) * 0.5).round(),
        (rect.y + (rect.height - glyph_h).max(0.0) * 0.5).round(),
    ];

    ui.push_clip(rect);
    if ui.widgets_sheet().is_some() {
        let (text_color, shadow_color) = if !enabled {
            ([0.627, 0.627, 0.627, 1.0], [0.157, 0.157, 0.157, 1.0])
        } else if let Some(tint) = tint {
            (tint, [tint[0] * 0.25, tint[1] * 0.25, tint[2] * 0.25, 1.0])
        } else if response.hovered {
            ([1.0, 1.0, 0.627, 1.0], [0.25, 0.25, 0.15, 1.0])
        } else {
            ([0.878, 0.878, 0.878, 1.0], [0.22, 0.22, 0.22, 1.0])
        };
        ui.draw().text(
            [position[0] + scale, position[1] + scale],
            label,
            TextStyle::new(scale, shadow_color),
        );
        ui.draw()
            .text(position, label, TextStyle::new(scale, text_color));
    } else {
        let text_color = if !enabled {
            style::TEXT_DIM
        } else {
            style::TEXT
        };
        ui.draw()
            .text(position, label, TextStyle::new(scale, text_color));
    }
    ui.pop_clip();

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::SpriteSheet;
    use crate::input::UiInput;

    #[test]
    fn button_with_skin_draws_images() {
        let viewport = Rect::new(0.0, 0.0, 320.0, 240.0);
        let mut ui = Ui::new(viewport);
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        ui.set_widgets_sheet(Some(sheet));
        ui.begin_frame(viewport, UiInput::default());

        let id = ui.id("test_btn");
        let btn_rect = Rect::new(10.0, 10.0, 200.0, 20.0);
        button(&mut ui, id, btn_rect, &FontAtlas::fallback(), "TEST");

        let output = ui.end_frame();
        let images = output
            .draw_list
            .commands()
            .iter()
            .filter(|cmd| matches!(cmd, crate::draw::DrawCommand::Image { .. }))
            .count();
        assert_eq!(images, 2);
    }
}
