// SPDX-License-Identifier: MIT OR Apache-2.0

//! The small bar at the bottom of the landing page: reset the selected widget
//! and reset all of them.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::Gui;
use crate::modern::theme::{RADIUS, SURFACE};
use crate::modern::ModernButton;
use crate::text::FontAtlas;

const BAR_HEIGHT: f32 = 22.0;
const BOTTOM_MARGIN: f32 = 10.0;
const PAD: f32 = 4.0;
const GAP: f32 = 4.0;
const BUTTON_HEIGHT: f32 = 14.0;
const WIDTHS: [f32; 2] = [66.0, 84.0];

/// What a button of the bar asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolbarAction {
    /// Reset the selected widget.
    Reset,
    /// Reset every widget.
    ResetAll,
}

/// Where the bar and its buttons sit, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ToolbarLayout {
    pub bar: Rect,
    pub buttons: [Rect; 2],
    pub scale: f32,
}

/// The top edge of the bar, in GUI units.
pub(super) fn top(gui: &Gui) -> f32 {
    gui.height - BOTTOM_MARGIN - BAR_HEIGHT
}

/// The bar of `viewport`: centred, above the bottom edge.
pub(super) fn layout(viewport: Rect) -> ToolbarLayout {
    let gui = Gui::new(viewport);
    let width = WIDTHS.iter().sum::<f32>() + GAP + PAD * 2.0;
    let left = ((gui.width - width) * 0.5).floor();
    let top = top(&gui);
    let mut x = left + PAD;
    let buttons = WIDTHS.map(|w| {
        let rect = gui.rect(
            x,
            top + (BAR_HEIGHT - BUTTON_HEIGHT) * 0.5,
            w,
            BUTTON_HEIGHT,
        );
        x += w + GAP;
        rect
    });
    ToolbarLayout {
        bar: gui.rect(left, top, width, BAR_HEIGHT),
        buttons,
        scale: gui.scale,
    }
}

/// Draws the bar and returns the button the player clicked. `enabled` says
/// whether there is a selected widget to reset and any widget at all.
pub(super) fn show(
    ui: &mut Ui,
    brand: &FontAtlas,
    lang: &Lang,
    layout: &ToolbarLayout,
    enabled: (bool, bool),
) -> Option<ToolbarAction> {
    ui.draw()
        .rounded_rect(layout.bar, RADIUS * layout.scale, SURFACE);
    let entries = [
        (
            "umicraft.mods.editor.reset",
            ToolbarAction::Reset,
            enabled.0,
        ),
        (
            "umicraft.mods.editor.reset_all",
            ToolbarAction::ResetAll,
            enabled.1,
        ),
    ];
    let mut clicked = None;
    for (rect, (key, action, on)) in layout.buttons.into_iter().zip(entries) {
        let id = ui.id(key);
        let fade = f32::from(u8::from(ui.is_hovered(rect)));
        let response = ModernButton::new(id, rect, lang.tr(key)).enabled(on).show(
            ui,
            brand,
            layout.scale,
            fade,
        );
        if response.clicked && on {
            clicked = Some(action);
        }
    }
    clicked
}
