// SPDX-License-Identifier: MIT OR Apache-2.0

//! The on/off switch: a rounded track with a knob that sits at one end.

use crate::context::Ui;
use crate::geom::Rect;
use crate::id::WidgetId;

use super::theme::{mix, ACCENT, SURFACE_HOVER, TEXT};

/// Width of the track, in GUI units.
pub const TOGGLE_WIDTH: f32 = 13.0;
/// Height of the track, in GUI units.
pub const TOGGLE_HEIGHT: f32 = 7.0;
/// Gap between the track and the knob, as a share of the track height.
const KNOB_INSET: f32 = 0.15;

/// The knob of a toggle whose track is `track`: a square at the right end when
/// `on`, at the left end when off.
#[must_use]
pub fn knob_rect(track: Rect, on: bool) -> Rect {
    knob_at(track, f32::from(u8::from(on)))
}

/// The knob `progress` of the way from the left end (`0.0`) to the right end
/// (`1.0`) of `track`.
#[must_use]
pub fn knob_at(track: Rect, progress: f32) -> Rect {
    knob_free(track, progress.clamp(0.0, 1.0))
}

/// [`knob_at`] without the clamp: a spring may carry the knob past an end.
fn knob_free(track: Rect, progress: f32) -> Rect {
    let inset = track.height * KNOB_INSET;
    let side = track.height - 2.0 * inset;
    let travel = track.width - 2.0 * inset - side;
    Rect::new(
        track.x + inset + travel * progress,
        track.y + inset,
        side,
        side,
    )
}

/// How far a knob may poke past the end of its travel, as a share of it: the
/// spring overshoots, the margin of the track takes it.
const OVERSHOOT: f32 = 0.2;
/// How much a knob stretches along its way per unit of speed, and at most.
const STRETCH_PER_SPEED: f32 = 0.09;
const MAX_STRETCH: f32 = 0.55;

/// The knob of a switch in motion: carried by a spring (`progress` may leave
/// `0.0..=1.0` a little) and stretched by its `speed`, per second, like a
/// drop that is pulled. It never leaves the track.
fn knob_moving(track: Rect, progress: f32, speed: f32) -> Rect {
    let mut knob = knob_free(track, progress.clamp(-OVERSHOOT, 1.0 + OVERSHOOT));
    let stretch = (speed.abs() * STRETCH_PER_SPEED).min(MAX_STRETCH);
    let grown = knob.width * stretch;
    knob.x -= grown * 0.5;
    knob.width += grown;
    let edge = track.height * KNOB_INSET * 0.4;
    let (left, right) = (track.x + edge, track.right() - edge);
    let x = knob.x.clamp(left, (right - knob.width).max(left));
    Rect::new(x, knob.y, knob.width, knob.height)
}

/// Draws a switch in `track` (physical pixels) and returns the state the
/// player asks for: `Some(!on)` on a click, `None` otherwise. The caller owns
/// the state and passes the new one back next frame.
///
/// `look` is the hover level, the position of the knob (`0.0` off, `1.0` on)
/// and its speed per second, all kept by the caller: the knob goes from one end
/// to the other and the track from grey to the accent colour as `progress`
/// does, and the knob stretches while it is fast.
pub fn toggle(
    ui: &mut Ui,
    id: WidgetId,
    track: Rect,
    on: bool,
    (hover, progress, speed): (f32, f32, f32),
) -> Option<bool> {
    let response = ui.response(id, track);
    let off = mix(SURFACE_HOVER, [0.30, 0.33, 0.40, 0.9], hover);
    let lit = mix(ACCENT, TEXT, hover * 0.15);
    let radius = track.height * 0.5;
    ui.draw()
        .rounded_rect(track, radius, mix(off, lit, progress));
    ui.draw()
        .rounded_rect(knob_moving(track, progress, speed), radius, TEXT);
    if response.clicked {
        ui.note_button_press();
        return Some(!on);
    }
    None
}
