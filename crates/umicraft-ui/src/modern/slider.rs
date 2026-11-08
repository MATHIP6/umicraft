// SPDX-License-Identifier: MIT OR Apache-2.0

//! The slider: a thin track, a filled part and a round knob, dragged with the
//! pointer. Values snap to the step of the [`SliderRange`].

use crate::context::Ui;
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::input::{InputEvent, PointerButton};

use super::theme::{ACCENT, SURFACE_HOVER, TEXT};

/// Share of the widget height the track takes.
const TRACK_SHARE: f32 = 0.3;

/// The values a slider can take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderRange {
    /// Smallest value.
    pub min: f32,
    /// Largest value.
    pub max: f32,
    /// Distance between two values; a step that is not positive means any value.
    pub step: f32,
}

impl SliderRange {
    /// Clamps `value` into the range and snaps it to the step, counted from
    /// `min` as the mods' settings are. A value that is not a number gives `min`.
    #[must_use]
    pub fn snap(self, value: f32) -> f32 {
        if !value.is_finite() {
            return self.min;
        }
        let stepped = if self.step > 0.0 {
            self.min + ((value - self.min) / self.step).round() * self.step
        } else {
            value
        };
        stepped.clamp(self.min, self.max.max(self.min))
    }

    /// Where `value` sits in the range, `0.0..=1.0`.
    #[must_use]
    pub fn fraction(self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span > 0.0 {
            ((value - self.min) / span).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The snapped value under pointer position `x` on a track spanning
    /// `[left, right]`.
    #[must_use]
    pub fn value_at(self, left: f32, right: f32, x: f32) -> f32 {
        let width = right - left;
        let fraction = if width > 0.0 {
            ((x - left) / width).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.snap(self.min + fraction * (self.max - self.min))
    }
}

/// Whether a drag of a slider is under way. Kept by the screen, so that a drag
/// goes on while the pointer leaves the slider and ends on the release.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SliderDrag(Option<WidgetId>);

impl SliderDrag {
    /// Whether the slider `id` is being dragged.
    #[must_use]
    pub fn is_dragging(self, id: WidgetId) -> bool {
        self.0 == Some(id)
    }
}

/// Draws the slider in `rect` (physical pixels) and returns the new value when
/// the player moved it to another step.
pub fn modern_slider(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    range: SliderRange,
    value: f32,
    drag: &mut SliderDrag,
) -> Option<f32> {
    let response = ui.response(id, rect);
    if response.clicked {
        ui.note_button_press();
    }
    let knob = rect.height * 0.5;
    let (left, right) = (rect.x + knob, rect.right() - knob);
    let current = range.snap(value);
    let mut next = current;
    for event in &ui.input().events {
        match event {
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            } if rect.contains(*position) => {
                drag.0 = Some(id);
                next = range.value_at(left, right, position[0]);
            }
            InputEvent::PointerMoved { position } if drag.is_dragging(id) => {
                next = range.value_at(left, right, position[0]);
            }
            InputEvent::PointerReleased {
                button: PointerButton::Left,
                ..
            } if drag.is_dragging(id) => drag.0 = None,
            _ => {}
        }
    }
    draw(ui, rect, range.fraction(next));
    ((next - current).abs() > f32::EPSILON).then_some(next)
}

fn draw(ui: &mut Ui, rect: Rect, fraction: f32) {
    let knob = rect.height * 0.5;
    let thickness = rect.height * TRACK_SHARE;
    let track = Rect::new(
        rect.x + knob,
        rect.y + (rect.height - thickness) * 0.5,
        rect.width - 2.0 * knob,
        thickness,
    );
    ui.draw()
        .rounded_rect(track, thickness * 0.5, SURFACE_HOVER);
    let filled = Rect::new(track.x, track.y, track.width * fraction, thickness);
    ui.draw().rounded_rect(filled, thickness * 0.5, ACCENT);
    let centre = track.x + track.width * fraction;
    let handle = Rect::new(
        centre - knob * 0.6,
        rect.y + rect.height * 0.2,
        knob * 1.2,
        rect.height * 0.6,
    );
    ui.draw().rounded_rect(handle, handle.width * 0.5, TEXT);
}
