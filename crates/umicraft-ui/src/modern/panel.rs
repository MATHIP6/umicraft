// SPDX-License-Identifier: MIT OR Apache-2.0

//! Panels and the screen vignette.

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;

use super::theme::{RADIUS, SURFACE, VIGNETTE};

/// Bands per vignette gradient; the draw list has no gradient primitive.
const VIGNETTE_BANDS: u32 = 8;
/// Share of the screen height the top gradient covers.
const TOP_SHARE: f32 = 0.3;
/// Share of the screen height the bottom gradient covers.
const BOTTOM_SHARE: f32 = 0.45;
/// Strength of the uniform tint under the gradients, relative to the vignette.
const BASE_STRENGTH: f32 = 0.35;
/// Strength of the gradients at the screen edge, relative to the vignette.
const EDGE_STRENGTH: f32 = 0.5;

/// Draws a translucent rounded panel; `scale` is pixels per GUI unit.
pub fn panel(ui: &mut Ui, rect: Rect, scale: f32) {
    ui.draw().rounded_rect(rect, RADIUS * scale, SURFACE);
}

/// Darkens `viewport` for the panorama: a light uniform tint, plus darker
/// bands fading out from the top and the bottom edges.
pub fn vignette(ui: &mut Ui, viewport: Rect) {
    ui.draw().rect(viewport, strength(BASE_STRENGTH));
    let top = Rect::new(
        viewport.x,
        viewport.y,
        viewport.width,
        viewport.height * TOP_SHARE,
    );
    fade_band(ui, top, true);
    let height = viewport.height * BOTTOM_SHARE;
    let bottom = Rect::new(
        viewport.x,
        viewport.y + viewport.height - height,
        viewport.width,
        height,
    );
    fade_band(ui, bottom, false);
}

/// The vignette colour at `fraction` of its strength.
fn strength(fraction: f32) -> Color {
    [
        VIGNETTE[0],
        VIGNETTE[1],
        VIGNETTE[2],
        VIGNETTE[3] * fraction,
    ]
}

/// Stacks bands inside `area` that are darkest at its top (`dark_at_top`) or
/// bottom edge.
fn fade_band(ui: &mut Ui, area: Rect, dark_at_top: bool) {
    let band = area.height / VIGNETTE_BANDS as f32;
    for index in 0..VIGNETTE_BANDS {
        let from_edge = if dark_at_top {
            index
        } else {
            VIGNETTE_BANDS - 1 - index
        };
        let fraction = 1.0 - from_edge as f32 / VIGNETTE_BANDS as f32;
        let rect = Rect::new(area.x, area.y + index as f32 * band, area.width, band);
        ui.draw().rect(rect, strength(fraction * EDGE_STRENGTH));
    }
}
