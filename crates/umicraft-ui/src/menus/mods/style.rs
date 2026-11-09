// SPDX-License-Identifier: MIT OR Apache-2.0

//! The look of the mods menu: colours, text sizes and the helpers every part
//! draws with. Where things go is in [`layout`](super::layout).

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::modern::theme::mix;
use crate::modern::{draw_text, Align};
use crate::text::FontAtlas;

/// A light dark sheet over the blurred game, so the menu reads well on bright
/// worlds.
pub(super) const BACKDROP: Color = [0.0, 0.0, 0.0, 0.25];
/// The panel behind everything: darker than a button so the cards stand out.
pub(super) const PANEL: Color = [0.085, 0.09, 0.10, 0.8];
/// The one-pixel line around the panel.
pub(super) const PANEL_RIM: Color = [1.0, 1.0, 1.0, 0.07];
/// Size of a mod name and of the title of a page, in GUI units.
pub(super) const NAME_SIZE: f32 = 6.0;
/// A row at rest: nothing, it is the panel.
pub(super) const CARD: Color = [1.0, 1.0, 1.0, 0.0];
/// A row under the pointer.
pub(super) const CARD_HOVER: Color = [1.0, 1.0, 1.0, 0.05];
/// A mod card at rest, under the pointer, and the tint a mod that is on adds: a
/// dark inset in the panel, not a light veil over it.
pub(super) const TILE: Color = [0.03, 0.035, 0.045, 0.55];
pub(super) const TILE_HOVER: Color = [0.075, 0.08, 0.095, 0.68];
pub(super) const TILE_ON: Color = [0.36, 0.55, 1.0, 0.07];

/// What every part needs to draw: the brand atlas and the pixels per GUI unit.
#[derive(Debug, Clone, Copy)]
pub(super) struct Paint<'a> {
    /// The atlas of the brand face.
    pub brand: &'a FontAtlas,
    /// Physical pixels per GUI unit.
    pub scale: f32,
}

impl Paint<'_> {
    /// Draws one line of text; `at` is the left (or aligned) edge and the
    /// vertical centre of the line, `size` is in GUI units.
    pub(super) fn text(
        self,
        ui: &mut Ui,
        text: &str,
        at: (f32, f32),
        (size, color, align): (f32, Color, Align),
    ) {
        draw_text(ui, self.brand, text, at, (size * self.scale, color, align));
    }
}

/// The hover level of a widget in `rect`: `1.0` under the pointer, else `0.0`.
/// The screen keeps no fade per widget, the menu is mostly static.
pub(super) fn hover_level(ui: &Ui, rect: Rect) -> f32 {
    f32::from(u8::from(ui.is_hovered(rect)))
}

/// The fill of a mod card: lifted under the pointer and tinted while the mod is
/// on. `hover` and `on` are levels, `0.0` to `1.0`.
pub(super) fn tile_fill(hover: f32, on: f32) -> Color {
    let base = mix(TILE, TILE_HOVER, hover);
    let tint = TILE_ON[3] * on.clamp(0.0, 1.0);
    // The accent is laid over the base: the alphas add up, the colour follows
    // the share each one holds.
    let alpha = base[3] + tint * (1.0 - base[3]);
    let share = if alpha > 0.0 {
        tint * (1.0 - base[3]) / alpha
    } else {
        0.0
    };
    [
        base[0] + (TILE_ON[0] - base[0]) * share,
        base[1] + (TILE_ON[1] - base[1]) * share,
        base[2] + (TILE_ON[2] - base[2]) * share,
        alpha,
    ]
}

/// The fill of a card or a row at the given hover level.
pub(super) fn card_fill(hover: f32) -> Color {
    mix(CARD, CARD_HOVER, hover)
}
