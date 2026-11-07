// SPDX-License-Identifier: MIT OR Apache-2.0

//! Panels and the screen vignette.

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;

use super::theme::{RADIUS, SURFACE, VIGNETTE};

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
/// gradients fading out from the top and the bottom edges.
pub fn vignette(ui: &mut Ui, viewport: Rect) {
    ui.draw().rect(viewport, strength(BASE_STRENGTH));
    let top = Rect::new(
        viewport.x,
        viewport.y,
        viewport.width,
        viewport.height * TOP_SHARE,
    );
    ui.draw()
        .vertical_gradient(top, strength(EDGE_STRENGTH), strength(0.0));
    let height = viewport.height * BOTTOM_SHARE;
    let bottom = Rect::new(
        viewport.x,
        viewport.y + viewport.height - height,
        viewport.width,
        height,
    );
    ui.draw()
        .vertical_gradient(bottom, strength(0.0), strength(EDGE_STRENGTH));
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
