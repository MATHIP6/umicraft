// SPDX-License-Identifier: MIT OR Apache-2.0

//! A row of tabs, one of them selected.

use crate::context::Ui;
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::text::{draw_text, measure_text, Align};
use super::theme::{mix, with_opacity, TEXT, TEXT_MUTED, TEXT_SIZE_SMALL};

/// Space on each side of a label, in GUI units.
const PADDING: f32 = 3.0;
/// Space between two tabs, in GUI units.
const GAP: f32 = 5.0;

/// The rectangles of tabs of the given `widths` laid side by side from the
/// left of `area`, all as high as `area`. `gap` separates them.
#[must_use]
pub fn tab_rects(area: Rect, widths: &[f32], gap: f32) -> Vec<Rect> {
    let mut x = area.x;
    widths
        .iter()
        .map(|&width| {
            let rect = Rect::new(x, area.y, width, area.height);
            x += width + gap;
            rect
        })
        .collect()
}

/// The index of the tab under `point`, if any.
#[must_use]
pub fn tab_at(rects: &[Rect], point: [f32; 2]) -> Option<usize> {
    rects.iter().position(|rect| rect.contains(point))
}

/// Draws the tabs in `area` (physical pixels) and returns the one clicked.
/// `scale` is pixels per GUI unit.
pub fn tabs(
    ui: &mut Ui,
    brand: &FontAtlas,
    area: Rect,
    labels: &[&str],
    (selected, position): (usize, f32),
    scale: f32,
) -> Option<usize> {
    let size = TEXT_SIZE_SMALL * scale;
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| measure_text(brand, label, size) + 2.0 * PADDING * scale)
        .collect();
    let rects = tab_rects(area, &widths, GAP * scale);
    ui.push_id("tabs");
    let mut clicked = None;
    for (index, (rect, label)) in rects.iter().zip(labels).enumerate() {
        let id = ui.id(&index.to_string());
        let response = ui.response(id, *rect);
        let hover = f32::from(u8::from(response.hovered));
        let color = if index == selected {
            TEXT
        } else {
            mix(with_opacity(TEXT_MUTED, 0.85), TEXT, hover)
        };
        let middle = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        draw_text(ui, brand, label, middle, (size, color, Align::Center));
        if response.clicked {
            ui.note_button_press();
            clicked = Some(index);
        }
    }
    ui.pop_id();
    // The line under the selected tab is a one-pixel rule that the caller moves
    // from tab to tab: `position` is a tab number that may be between two.
    if let Some(last) = rects.len().checked_sub(1) {
        let at = position.clamp(0.0, last as f32);
        let (from, to) = (at.floor() as usize, (at.floor() as usize + 1).min(last));
        let blend = at - at.floor();
        let lerp = |a: f32, b: f32| a + (b - a) * blend;
        let thickness = scale.max(1.0);
        let line = Rect::new(
            lerp(rects[from].x, rects[to].x) + PADDING * scale,
            area.bottom() - thickness,
            lerp(rects[from].width, rects[to].width) - 2.0 * PADDING * scale,
            thickness,
        );
        ui.draw().rect(line, with_opacity(TEXT, 0.85));
    }
    clicked
}
