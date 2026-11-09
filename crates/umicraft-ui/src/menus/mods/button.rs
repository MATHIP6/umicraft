// SPDX-License-Identifier: MIT OR Apache-2.0

//! The icon-only buttons of the panel: the close cross and the settings of a row.

use crate::context::{Ui, UiResponse};
use crate::geom::Rect;
use crate::icons::Icon;
use crate::id::WidgetId;
use crate::modern::theme::{mix, with_opacity};
use crate::modern::{draw_icon, theme};
use crate::text::FontAtlas;

/// A square button with only an icon, for the close cross and the settings of a
/// row: a faint tile that appears under the pointer, the icon brighter. `fade`
/// is the hover level, `0.0` to `1.0`.
pub(super) fn icon_button(
    ui: &mut Ui,
    brand: &FontAtlas,
    target: (WidgetId, Rect),
    (icon, scale, fade): (Icon, f32, f32),
) -> UiResponse {
    let (id, rect) = target;
    let response = ui.response(id, rect);
    if response.clicked {
        ui.note_button_press();
    }
    let tile = with_opacity(theme::TEXT, 0.14 * fade);
    ui.draw().rounded_rect(rect, theme::RADIUS * scale, tile);
    let side = rect.width.min(rect.height) * 0.85;
    let centre = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
    let colour = mix(theme::TEXT_MUTED, theme::TEXT, fade);
    draw_icon(ui, brand, icon, centre, (side, colour));
    response
}
