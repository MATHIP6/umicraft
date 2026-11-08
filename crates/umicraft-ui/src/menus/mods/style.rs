// SPDX-License-Identifier: MIT OR Apache-2.0

//! The look of the mods menu: colours, text sizes and the helpers every part
//! draws with. Where things go is in [`layout`](super::layout) and
//! [`landing_layout`](super::landing_layout).

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::modern::theme::{self, mix};
use crate::modern::{draw_text, Align};
use crate::text::FontAtlas;

/// A light dark sheet over the blurred game, so the menu reads well on bright
/// worlds.
pub(super) const BACKDROP: Color = [0.0, 0.0, 0.0, 0.25];
/// The panel behind everything: darker than a button so the cards stand out.
pub(super) const PANEL: Color = [0.05, 0.06, 0.08, 0.86];
/// Thickness of the thin rim of the Mods button, in GUI units.
pub(super) const RIM_WIDTH: f32 = 0.5;
/// The rim at rest: a faint white line.
pub(super) const RIM: Color = [1.0, 1.0, 1.0, 0.14];
/// The rim when the pointer is over the button: the accent colour.
pub(super) const RIM_HOVER: Color = [0.36, 0.55, 1.0, 0.70];
/// Edge of the module grid icon, in GUI units: two squares and a gap.
pub(super) const ICON_SIZE: f32 = 7.0;
/// Space between the icon and the label.
pub(super) const ICON_GAP: f32 = 5.0;
/// Space between the capitals of the label, as a share of their size.
pub(super) const LABEL_TRACKING: f32 = 0.25;
/// The label of the close button, as on the title screen.
pub(super) const CLOSE_CROSS: &str = "\u{d7}";

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

/// The fill of a card or a row at the given hover level.
pub(super) fn card_fill(hover: f32) -> Color {
    mix(theme::SURFACE, theme::SURFACE_HOVER, hover)
}
