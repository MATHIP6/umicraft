// SPDX-License-Identifier: MIT OR Apache-2.0

//! A row of tabs, one of them selected.

use crate::context::Ui;
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::text::{draw_text, fit_text, measure_text, Align};
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

/// The widths of tabs of natural `widths` once they fit in `available` with `gap`
/// between them: when they do not, the widest are cut down to one common width, so
/// short names stay whole and only long ones are shortened.
#[must_use]
pub fn fit_widths(widths: &[f32], available: f32, gap: f32) -> Vec<f32> {
    let room = available - gap * widths.len().saturating_sub(1) as f32;
    if widths.iter().sum::<f32>() <= room {
        return widths.to_vec();
    }
    let mut sorted = widths.to_vec();
    sorted.sort_by(f32::total_cmp);
    // Raise the cap tab by tab: every tab narrower than the cap keeps its width,
    // the others share what is left.
    let mut used = 0.0;
    let mut cap = 0.0;
    for (index, width) in sorted.iter().enumerate() {
        let share = (room - used) / (sorted.len() - index) as f32;
        cap = share.max(0.0);
        if *width > share {
            break;
        }
        used += width;
    }
    widths.iter().map(|width| width.min(cap)).collect()
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
    let natural: Vec<f32> = labels
        .iter()
        .map(|label| measure_text(brand, label, size) + 2.0 * PADDING * scale)
        .collect();
    let widths = fit_widths(&natural, area.width, GAP * scale);
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
        let shown = fit_text(brand, label, size, rect.width - 2.0 * PADDING * scale);
        draw_text(ui, brand, &shown, middle, (size, color, Align::Center));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_that_fit_keep_their_width() {
        assert_eq!(fit_widths(&[10.0, 20.0], 40.0, 5.0), [10.0, 20.0]);
    }

    #[test]
    fn only_the_widest_tabs_are_cut_to_fit() {
        let widths = fit_widths(&[10.0, 40.0, 60.0], 70.0, 5.0);
        assert_eq!(widths, [10.0, 25.0, 25.0]);
        let total: f32 = widths.iter().sum::<f32>() + 10.0;
        assert!(total <= 70.0 + f32::EPSILON);
    }

    #[test]
    fn no_room_at_all_gives_empty_tabs_not_negative_ones() {
        assert!(fit_widths(&[10.0, 10.0], 2.0, 5.0)
            .iter()
            .all(|w| *w == 0.0));
    }
}
