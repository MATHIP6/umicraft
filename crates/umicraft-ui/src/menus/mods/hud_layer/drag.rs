// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mouse logic of the HUD layer, free of any drawing: hit tests, the drag
//! of a widget (move or resize by its corner) and the scale steps of the wheel.
//! Everything is in GUI units.

use crate::geom::Rect;

use super::frame::frame_rect;
use super::guides::{guides_for, Guide};
use super::view::Snap;

/// What one notch of the wheel changes the scale by. The corner handle has no
/// step: the scale follows the pointer to any value.
pub(super) const SCALE_STEP: f32 = 0.05;
/// Edge of the corner handle that resizes a widget, in GUI units.
pub(super) const HANDLE: f32 = 4.0;
/// How far outside its square the handle still catches the pointer.
const HANDLE_REACH: f32 = 1.5;

/// The limits a drag keeps to.
#[derive(Clone, Copy)]
pub(super) struct Bounds<'a> {
    /// The screen size, in GUI units.
    pub screen: [f32; 2],
    /// The smallest and the largest widget scale.
    pub scale_range: (f32, f32),
    /// The snapping applied while a widget moves.
    pub snap: Snap<'a>,
}

/// The corner handle of a widget, drawn inside the bottom-right corner of its
/// frame.
pub(super) fn handle_rect(rect: Rect) -> Rect {
    Rect::new(
        rect.right() - HANDLE.min(rect.width),
        rect.bottom() - HANDLE.min(rect.height),
        HANDLE.min(rect.width),
        HANDLE.min(rect.height),
    )
}

/// Whether `point` is on the corner handle of `rect`.
pub(super) fn on_handle(rect: Rect, point: [f32; 2]) -> bool {
    let handle = handle_rect(rect);
    point[0] >= handle.x - HANDLE_REACH
        && point[0] <= handle.right() + HANDLE_REACH
        && point[1] >= handle.y - HANDLE_REACH
        && point[1] <= handle.bottom() + HANDLE_REACH
}

/// The topmost widget under `point`: the last of `rects` that contains it or
/// its handle.
pub(super) fn hit_widget(rects: &[Rect], point: [f32; 2], screen: [f32; 2]) -> Option<usize> {
    rects.iter().rposition(|rect| {
        let frame = frame_rect(*rect, screen);
        frame.contains(point) || on_handle(frame, point)
    })
}

/// `scale` kept in `range`, whatever its value.
pub(super) fn clamp_scale(scale: f32, range: (f32, f32)) -> f32 {
    scale.clamp(range.0, range.1)
}

/// `rect` moved back inside the screen; one that is larger than the screen
/// sits at the origin, like `HudPlacement::rect` places it.
pub(super) fn clamp_into(rect: Rect, screen: [f32; 2]) -> Rect {
    let fit = |pos: f32, size: f32, total: f32| {
        if size >= total {
            0.0
        } else {
            pos.clamp(0.0, total - size)
        }
    };
    Rect::new(
        fit(rect.x, rect.width, screen[0]),
        fit(rect.y, rect.height, screen[1]),
        rect.width,
        rect.height,
    )
}

/// `rect` at scale `to` instead of `from`, its top-left corner staying put
/// unless the screen edge pushes it back.
fn rescaled(rect: Rect, (from, to): (f32, f32), screen: [f32; 2]) -> Rect {
    let factor = to / from;
    clamp_into(
        Rect::new(rect.x, rect.y, rect.width * factor, rect.height * factor),
        screen,
    )
}

/// The rectangle and scale a widget gets from `notches` of the wheel (positive
/// is down, which shrinks it), or `None` when the scale does not change.
pub(super) fn wheel_resize(
    (rect, scale): (Rect, f32),
    notches: f32,
    bounds: &Bounds<'_>,
) -> Option<(Rect, f32)> {
    let next = clamp_scale(scale - notches * SCALE_STEP, bounds.scale_range);
    ((next - scale).abs() > f32::EPSILON)
        .then(|| (rescaled(rect, (scale, next), bounds.screen), next))
}

/// What a drag moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Grab {
    /// The whole widget; the pointer holds it at this offset from its corner.
    Move([f32; 2]),
    /// The corner handle: the scale follows the pointer, which holds the corner
    /// this far (in x) from where it pressed.
    Resize(f32),
}

/// A drag under way: where the widget started and where it is now.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Drag {
    /// The id of the mod whose widget is dragged.
    pub id: String,
    grab: Grab,
    start: (Rect, f32),
    /// The rectangle the widget has now.
    pub live: Rect,
    /// The scale the widget has now.
    pub scale: f32,
    /// The guides the widget lies on now.
    pub guides: Vec<Guide>,
}

impl Drag {
    /// Starts dragging the widget of `id`, which covers `rect` at `scale`.
    pub(super) fn begin(id: String, grab: Grab, rect: Rect, scale: f32) -> Self {
        Self {
            id,
            grab,
            start: (rect, scale),
            live: rect,
            scale,
            guides: Vec::new(),
        }
    }

    /// Follows the pointer to `point`.
    pub(super) fn follow(&mut self, point: [f32; 2], bounds: &Bounds<'_>) {
        let (start, start_scale) = self.start;
        match self.grab {
            Grab::Move([dx, dy]) => {
                let raw = Rect::new(point[0] - dx, point[1] - dy, start.width, start.height);
                let snapped = (bounds.snap)(clamp_into(raw, bounds.screen), bounds.screen);
                self.live = clamp_into(snapped, bounds.screen);
                self.guides = guides_for(self.live, bounds.screen);
            }
            Grab::Resize(held) => {
                let unit_width = start.width / start_scale;
                let width = point[0] - held - start.x;
                let scale = clamp_scale(width / unit_width, bounds.scale_range);
                self.live = rescaled(start, (start_scale, scale), bounds.screen);
                self.scale = scale;
            }
        }
    }

    /// Whether the widget ended up somewhere else than it started.
    pub(super) fn changed(&self) -> bool {
        (self.live, self.scale) != self.start
    }
}
