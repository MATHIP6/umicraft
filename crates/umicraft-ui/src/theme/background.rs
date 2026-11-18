// SPDX-License-Identifier: MIT OR Apache-2.0

//! The screen behind the menus.

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::modern::theme::mix;

/// What a menu without a world behind it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundSkin {
    /// The tiled dirt of 1.8 (a flat grey without the texture).
    Dirt,
    /// A vertical gradient over the whole viewport; lists get a translucent
    /// `list` panel instead of the dirt container.
    Gradient {
        /// Colour of the top edge of the viewport.
        top: Color,
        /// Colour of the bottom edge of the viewport.
        bottom: Color,
        /// Fill of a list container, drawn over the gradient.
        list: Color,
    },
}

/// The part of a viewport-wide gradient that lies behind `area`, as the
/// `(top, bottom)` colours of that area, so a patch matches its surroundings.
#[must_use]
pub(crate) fn gradient_slice(
    viewport: Rect,
    area: Rect,
    (top, bottom): (Color, Color),
) -> (Color, Color) {
    let at = |y: f32| mix(top, bottom, (y - viewport.y) / viewport.height.max(1.0));
    (at(area.y), at(area.bottom()))
}

/// Paints `area` with the matching slice of the gradient of `ui`'s viewport.
pub(crate) fn fill_gradient(ui: &mut Ui, area: Rect, edges: (Color, Color)) {
    if area.is_empty() {
        return;
    }
    let (top, bottom) = gradient_slice(ui.viewport(), area, edges);
    ui.draw().vertical_gradient(area, top, bottom);
}
