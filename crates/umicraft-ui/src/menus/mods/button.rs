// SPDX-License-Identifier: MIT OR Apache-2.0

//! The buttons of the menu: the Mods button of the landing page and the
//! icon-only ones of the panel (the close cross and the settings of a row).

use crate::context::{Ui, UiResponse};
use crate::geom::Rect;
use crate::icons::Icon;
use crate::id::WidgetId;
use crate::modern::theme::{mix, with_opacity};
use crate::modern::{draw_icon, draw_text, measure_text, theme, Align};
use crate::text::FontAtlas;

use super::style::{
    bordered, GLASS, GLASS_BORDER, GLASS_HIGHLIGHT, GLASS_HOVER, ICON_GAP, ICON_SIZE,
    LABEL_TRACKING,
};

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

/// Draws the Mods button and reports its interaction state: a glass pill, a
/// faint fill with a thin light border and a brighter line along its top
/// edge, a gear and the label in tracked capitals, centred together.
///
/// `target` is the widget id and the rectangle, `look` the GUI scale and the
/// hover level of the button.
pub(super) fn mods_button(
    ui: &mut Ui,
    brand: &FontAtlas,
    target: (WidgetId, Rect),
    look: (f32, f32),
    label: &str,
) -> UiResponse {
    let (id, rect) = target;
    let (scale, fade) = look;
    let response = ui.response(id, rect);
    if response.clicked {
        ui.note_button_press();
    }

    let radius = 7.0 * scale;
    bordered(
        ui,
        rect,
        radius,
        (mix(GLASS, GLASS_HOVER, fade), GLASS_BORDER),
    );
    // The inner highlight: one pixel under the top border, between the corners.
    let inset = radius.min(rect.width * 0.5);
    let highlight = Rect::new(rect.x + inset, rect.y + 1.0, rect.width - 2.0 * inset, 1.0);
    ui.draw().rect(highlight, GLASS_HIGHLIGHT);

    let label = label.to_uppercase();
    let size = theme::TEXT_SIZE_BODY * scale;
    let gap = size * LABEL_TRACKING;
    let mut buffer = [0u8; 4];
    let glyph_widths: Vec<f32> = label
        .chars()
        .map(|letter| measure_text(brand, letter.encode_utf8(&mut buffer), size))
        .collect();
    let text_width =
        glyph_widths.iter().sum::<f32>() + gap * label.chars().count().saturating_sub(1) as f32;
    let icon = ICON_SIZE * scale;
    let group = icon + ICON_GAP * scale + text_width;
    let left = rect.x + (rect.width - group) * 0.5;
    let middle = rect.y + rect.height * 0.5;

    let icon_centre = (left + icon * 0.5, middle);
    draw_icon(ui, brand, Icon::Gear, icon_centre, (icon, theme::TEXT));
    let mut x = left + icon + ICON_GAP * scale;
    for (letter, width) in label.chars().zip(glyph_widths) {
        let glyph = letter.encode_utf8(&mut buffer);
        draw_text(
            ui,
            brand,
            glyph,
            (x, middle),
            (size, theme::TEXT, Align::Left),
        );
        x += width + gap;
    }
    response
}
