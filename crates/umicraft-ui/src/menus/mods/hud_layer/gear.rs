// SPDX-License-Identifier: MIT OR Apache-2.0

//! The gear on a widget of the HUD layer: a small icon in a strip the frame of the
//! widget leaves under its text (above it when the widget is at the bottom of the
//! screen), at the left. It shows while the pointer is
//! over the widget or the widget is selected, and opens the settings of its mod.

use crate::context::Ui;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::menus::common::Gui;
use crate::modern::draw_icon;
use crate::modern::theme::{with_opacity, TEXT};
use crate::text::FontAtlas;

/// The edge of the gear as a share of the smaller side of the widget.
const SIZE_SHARE: f32 = 0.16;
/// The smallest gear, in GUI units, so it stays readable on a one-line widget.
const MIN_SIDE: f32 = 4.0;
/// Space around the gear in its strip, as a share of the gear.
const INSET_SHARE: f32 = 0.35;
/// How visible the gear is at rest and under the pointer.
const REST: f32 = 0.45;
const HOVER: f32 = 1.0;

/// The edge of the gear of the widget at `rect`, in GUI units.
fn side(rect: Rect) -> f32 {
    (SIZE_SHARE * rect.width.min(rect.height)).max(MIN_SIDE)
}

/// The height of the strip next to the text that holds the gear: the frame of the
/// widget is this much taller than its text, so the gear never covers a line.
pub(super) fn strip(rect: Rect) -> f32 {
    side(rect) * (1.0 + 2.0 * INSET_SHARE)
}

/// Whether the strip goes under the text: it does unless the widget is so low on
/// a screen of `screen` GUI units that the strip would fall off its bottom, and
/// then it goes above the text.
pub(super) fn strip_below(rect: Rect, screen: [f32; 2]) -> bool {
    rect.bottom() + super::frame::MARGIN + strip(rect) <= screen[1]
}

/// The gear of the widget at `rect` (GUI units): in the strip of the frame, at its
/// left, away from the resize handle on the right.
pub(super) fn gear_rect(rect: Rect, screen: [f32; 2]) -> Rect {
    let side = side(rect);
    let inset = INSET_SHARE * side;
    let frame = super::frame::frame_rect(rect, screen);
    let y = if strip_below(rect, screen) {
        frame.bottom() - side - inset
    } else {
        frame.y + inset
    };
    Rect::new(frame.x + inset, y, side, side)
}

/// Draws the gear at `rect` (GUI units): the icon alone, no plate behind it;
/// `hovered` is whether the pointer is on it.
pub(super) fn draw_gear(ui: &mut Ui, brand: &FontAtlas, gui: &Gui, rect: Rect, hovered: bool) {
    let area = gui.rect(rect.x, rect.y, rect.width, rect.height);
    let centre = (area.x + area.width * 0.5, area.y + area.height * 0.5);
    let opacity = if hovered { HOVER } else { REST };
    draw_icon(
        ui,
        brand,
        Icon::Gear,
        centre,
        (area.width.min(area.height), with_opacity(TEXT, opacity)),
    );
}
