// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shade and the arrows over the 32x32 head of a hovered list row: play,
//! up and down (`ServerListEntryNormal.drawEntry`, `server_selection.png`).
//!
//! Shared by the server list and the account list so both look and behave the
//! same. A sheet the atlas lacks falls back to text glyphs.

use crate::context::Ui;
use crate::draw::Color;
use crate::text::FontAtlas;

use super::common::{Gui, TextPlacement, WHITE};

/// `Gui.drawRect(x, y, x + 32, y + 32, -1601138544)` = `0xA0909090`.
const HOVER_SHADE: Color = [0.565, 0.565, 0.565, 0.627];

/// Which arrows a hovered row shows and where the pointer is.
#[derive(Debug, Clone, Copy)]
pub(super) struct HoverArrows {
    /// The row can move up (it is not the first).
    pub can_up: bool,
    /// The row can move down (it is not the last).
    pub can_down: bool,
    /// The pointer relative to the row's corner, in GUI units: the arrow under
    /// it is lit.
    pub rel: [f32; 2],
}

/// Draws the hover shade over the head at GUI `at` and the arrows `arrows`
/// allows; the play arrow is always there.
pub(super) fn draw_hover_arrows(
    ui: &mut Ui,
    (gui, atlas): (&Gui, &FontAtlas),
    at: [f32; 2],
    arrows: HoverArrows,
) {
    let rect = gui.rect(at[0], at[1], 32.0, 32.0);
    ui.draw().rect(rect, HOVER_SHADE);
    let rel = arrows.rel;
    let join_hot = rel[0] < 32.0 && rel[0] > 16.0;
    let up_hot = rel[0] < 16.0 && rel[1] < 16.0;
    let down_hot = rel[0] < 16.0 && rel[1] > 16.0;
    let cells = [
        (true, 0, join_hot, ">", [21.0, 12.0]),
        (arrows.can_up, 96, up_hot, "^", [5.0, 3.0]),
        (arrows.can_down, 64, down_hot, "v", [5.0, 21.0]),
    ];
    let sheet = ui.menu_art().server_selection;
    for (_, u, hot, glyph, offset) in cells.into_iter().filter(|cell| cell.0) {
        match sheet {
            Some(sheet) => {
                let v = if hot { 32 } else { 0 };
                ui.draw().image(sheet.sub(u, v, 32, 32), rect, [1.0; 4]);
            }
            None => gui.text(
                ui,
                atlas,
                glyph,
                TextPlacement::left([at[0] + offset[0], at[1] + offset[1]], WHITE),
            ),
        }
    }
}
