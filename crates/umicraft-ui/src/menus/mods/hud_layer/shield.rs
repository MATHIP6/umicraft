// SPDX-License-Identifier: MIT OR Apache-2.0

//! The parts of the screen where the HUD layer leaves the pointer to the rest
//! of the menu: a press there never grabs a widget and the wheel never resizes
//! one, so the Mods button, the cross and the open panel keep their clicks.

use crate::geom::Rect;

/// The rectangles, in physical pixels, that the layer stays out of.
#[derive(Debug, Clone, Copy)]
pub(super) struct Shield<'a> {
    blocked: &'a [Rect],
    bar: Option<Rect>,
}

impl<'a> Shield<'a> {
    /// A shield over `blocked` and, when the reset bar is shown, over `bar`.
    pub(super) fn new(blocked: &'a [Rect], bar: Option<Rect>) -> Self {
        Self { blocked, bar }
    }

    /// Whether `point` (physical pixels) is on a protected part.
    pub(super) fn covers(&self, point: [f32; 2]) -> bool {
        self.blocked
            .iter()
            .chain(self.bar.as_ref())
            .any(|rect| rect.contains(point))
    }
}
