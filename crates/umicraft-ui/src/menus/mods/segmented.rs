// SPDX-License-Identifier: MIT OR Apache-2.0

//! A segmented control: a few labels side by side in one rounded box, the
//! selected one lifted.

use crate::context::Ui;
use crate::geom::Rect;
use crate::modern::theme::{self, with_opacity};
use crate::modern::{fit_text, Align};

use super::style::{bordered, Paint, FIELD_BORDER, SURFACE_3};

/// Space between the box and its segments, in GUI units.
const PAD: f32 = 1.5;
/// Size of the labels.
const LABEL_SIZE: f32 = 6.5;

/// Draws `labels` as the segments of a control filling `rect`, the one at
/// index `selected` lit, and returns the index of the segment clicked, if it
/// is not the selected one.
pub(super) fn segmented(
    ui: &mut Ui,
    paint: Paint<'_>,
    rect: Rect,
    labels: &[&str],
    selected: usize,
) -> Option<usize> {
    let scale = paint.scale;
    bordered(ui, rect, 5.5 * scale, ([1.0, 1.0, 1.0, 0.05], FIELD_BORDER));
    let inner = Rect::new(
        rect.x + PAD * scale,
        rect.y + PAD * scale,
        rect.width - 2.0 * PAD * scale,
        rect.height - 2.0 * PAD * scale,
    );
    let width = inner.width / labels.len().max(1) as f32;
    let mut clicked = None;
    ui.push_id("segmented");
    for (index, label) in labels.iter().enumerate() {
        let segment = Rect::new(inner.x + index as f32 * width, inner.y, width, inner.height);
        let id = ui.id(label);
        let response = ui.response(id, segment);
        if response.clicked {
            ui.note_button_press();
            if index != selected {
                clicked = Some(index);
            }
        }
        let active = index == selected;
        if active {
            ui.draw().rounded_rect(segment, 4.0 * scale, SURFACE_3);
        }
        let colour = if active {
            theme::TEXT
        } else {
            with_opacity(theme::TEXT, 0.7)
        };
        let text = fit_text(paint.brand, label, LABEL_SIZE * scale, width - 4.0 * scale);
        let middle = (segment.x + width * 0.5, segment.y + segment.height * 0.5);
        paint.text(ui, &text, middle, (LABEL_SIZE, colour, Align::Center));
    }
    ui.pop_id();
    clicked
}
