// SPDX-License-Identifier: MIT OR Apache-2.0

//! The on/off switch: a rounded track with a knob that sits at one end.

use crate::context::Ui;
use crate::geom::Rect;
use crate::id::WidgetId;

use super::theme::{mix, ACCENT, SURFACE_HOVER, TEXT};

/// Width of the track, in GUI units.
pub const TOGGLE_WIDTH: f32 = 16.0;
/// Height of the track, in GUI units.
pub const TOGGLE_HEIGHT: f32 = 8.0;
/// Gap between the track and the knob, as a share of the track height.
const KNOB_INSET: f32 = 0.15;

/// The knob of a toggle whose track is `track`: a square at the right end when
/// `on`, at the left end when off.
#[must_use]
pub fn knob_rect(track: Rect, on: bool) -> Rect {
    let inset = track.height * KNOB_INSET;
    let side = track.height - 2.0 * inset;
    let x = if on {
        track.right() - inset - side
    } else {
        track.x + inset
    };
    Rect::new(x, track.y + inset, side, side)
}

/// Draws a switch in `track` (physical pixels) and returns the state the
/// player asks for: `Some(!on)` on a click, `None` otherwise. The caller owns
/// the state and passes the new one back next frame.
///
/// `hover` is the fade level `0.0..=1.0` the caller keeps for this widget.
pub fn toggle(ui: &mut Ui, id: WidgetId, track: Rect, on: bool, hover: f32) -> Option<bool> {
    let response = ui.response(id, track);
    let off = mix(SURFACE_HOVER, [0.30, 0.33, 0.40, 0.9], hover);
    let color = if on {
        mix(ACCENT, TEXT, hover * 0.15)
    } else {
        off
    };
    let radius = track.height * 0.5;
    ui.draw().rounded_rect(track, radius, color);
    ui.draw().rounded_rect(knob_rect(track, on), radius, TEXT);
    if response.clicked {
        ui.note_button_press();
        return Some(!on);
    }
    None
}
