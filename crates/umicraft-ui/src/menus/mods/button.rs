// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Mods button: a dark rounded card with a thin rim, a grid icon and the
//! label in tracked capitals.

use crate::context::{Ui, UiResponse};
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::modern::theme::{mix, with_opacity};
use crate::modern::{draw_text, measure_text, theme, Align};
use crate::text::FontAtlas;

use super::style::{ICON_GAP, ICON_SIZE, LABEL_TRACKING, RIM, RIM_HOVER, RIM_WIDTH};

/// Draws the Mods button and reports its interaction state: a dark rounded
/// card with a thin rim that lights up in the accent colour under the
/// pointer, a small grid of modules and the label in tracked capitals.
///
/// `target` is the widget id and the rectangle, `look` the GUI scale and the
/// hover fade of the button.
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

    // The rim is the whole card in its colour; the fill goes over it, one rim
    // width in from every edge.
    let radius = theme::RADIUS * scale;
    let rim = mix(RIM, RIM_HOVER, fade);
    ui.draw().rounded_rect(rect, radius, rim);
    let inset = (RIM_WIDTH * scale).max(1.0);
    let inner = Rect::new(
        rect.x + inset,
        rect.y + inset,
        rect.width - 2.0 * inset,
        rect.height - 2.0 * inset,
    );
    let fill = mix(theme::SURFACE, theme::SURFACE_HOVER, fade);
    ui.draw()
        .rounded_rect(inner, (radius - inset).max(0.0), fill);

    // The icon and the label are centred together.
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

    let cell = icon * 3.0 / 7.0;
    let step = icon - cell;
    let icon_colour = with_opacity(theme::TEXT, 0.9);
    for (column, row) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let square = Rect::new(
            left + column * step,
            middle - icon * 0.5 + row * step,
            cell,
            cell,
        );
        ui.draw().rounded_rect(square, cell * 0.25, icon_colour);
    }
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
