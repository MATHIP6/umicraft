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
/// The panel behind everything (`#101014` at 94 %: the design asks for 78 %, but
/// the game showed through too much).
pub(super) const PANEL: Color = [0.063, 0.063, 0.078, 0.94];
/// The one-pixel line around the panel.
pub(super) const PANEL_RIM: Color = [1.0, 1.0, 1.0, 0.09];
/// The sidebar, a faint veil over the panel.
pub(super) const SIDEBAR: Color = [1.0, 1.0, 1.0, 0.02];
/// Lines between the sidebar and the page, and around cards and groups.
pub(super) const DIVIDER: Color = [1.0, 1.0, 1.0, 0.07];
/// Lines between the rows of a settings group.
pub(super) const ROW_DIVIDER: Color = [1.0, 1.0, 1.0, 0.06];
/// Cards and settings groups.
pub(super) const SURFACE_1: Color = [1.0, 1.0, 1.0, 0.035];
/// Small buttons, tags and the icon tile of a mod that is off.
pub(super) const SURFACE_2: Color = [1.0, 1.0, 1.0, 0.06];
/// The active segment of a segmented control.
pub(super) const SURFACE_3: Color = [1.0, 1.0, 1.0, 0.12];
/// The search field and the segmented control: their border.
pub(super) const FIELD_BORDER: Color = [1.0, 1.0, 1.0, 0.10];
/// A sidebar item under the pointer.
pub(super) const ITEM_HOVER: Color = [1.0, 1.0, 1.0, 0.04];
/// The active sidebar item: the accent at 22 %.
pub(super) const ACCENT_SOFT: Color = [0.486, 0.227, 0.929, 0.22];
/// An enabled card: the accent at 10 % and its border at 55 %.
pub(super) const ACCENT_TINT: Color = [0.486, 0.227, 0.929, 0.10];
pub(super) const ACCENT_BORDER: Color = [0.486, 0.227, 0.929, 0.55];
/// The accent of a button under the pointer (`#8B5CF6`).
pub(super) const ACCENT_HOVER: Color = [0.545, 0.361, 0.965, 1.0];
/// The glass button of the landing page: fill, border and its lit border.
pub(super) const GLASS: Color = [1.0, 1.0, 1.0, 0.08];
pub(super) const GLASS_HOVER: Color = [1.0, 1.0, 1.0, 0.13];
pub(super) const GLASS_BORDER: Color = [1.0, 1.0, 1.0, 0.22];
pub(super) const GLASS_HIGHLIGHT: Color = [1.0, 1.0, 1.0, 0.12];
/// Size of a name in the HUD editor, in GUI units.
pub(super) const NAME_SIZE: f32 = 6.0;
/// Size of a mod name on a card and of a page title (15 and 20 px at the
/// reference size of the design).
pub(super) const MOD_NAME_SIZE: f32 = 7.5;
pub(super) const TITLE_SIZE: f32 = 10.0;
/// Edge of the gear icon of the Mods button, in GUI units.
pub(super) const ICON_SIZE: f32 = 9.0;
/// Gap between that icon and the label.
pub(super) const ICON_GAP: f32 = 6.0;
/// Space between the capitals of the label, as a share of their size.
pub(super) const LABEL_TRACKING: f32 = 0.18;
/// Size of a description (12.5 px).
pub(super) const DESC_SIZE: f32 = 6.25;
/// Size of a section label, a tag and a count (11 px).
pub(super) const LABEL_SIZE: f32 = 5.5;

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

/// Draws `rect` with a one-pixel `border` around its `fill`.
pub(super) fn bordered(ui: &mut Ui, rect: Rect, radius: f32, (fill, border): (Color, Color)) {
    let rim = 1.0;
    ui.draw().rounded_rect(rect, radius, border);
    let inner = Rect::new(
        rect.x + rim,
        rect.y + rim,
        rect.width - 2.0 * rim,
        rect.height - 2.0 * rim,
    );
    ui.draw().rounded_rect(inner, (radius - rim).max(0.0), fill);
}

/// The fill and the border of a mod card at the given hover and on levels
/// (`0.0` to `1.0`): lifted under the pointer, tinted with the accent while
/// the mod is on.
pub(super) fn card_colors(hover: f32, on: f32) -> (Color, Color) {
    let lifted = mix(SURFACE_1, [1.0, 1.0, 1.0, 0.06], hover);
    (
        mix(lifted, ACCENT_TINT, on),
        mix(DIVIDER, ACCENT_BORDER, on),
    )
}
