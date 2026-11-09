// SPDX-License-Identifier: MIT OR Apache-2.0

//! The view model and the drawing of one account row.
//!
//! Heads: the client supplies the 8x8 face of each player's skin (the UI crate
//! cannot load skins); without one a flat square keeps the slot aligned.

use std::sync::Arc;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::Color;
use crate::menus::common::{Gui, TextPlacement, DIM_GRAY, WHITE};
use crate::text::FontAtlas;

use super::face::draw_face;
use super::layout::{ROW_HEIGHT, ROW_WIDTH};

/// The green of the offline badge (`§a`).
const OFFLINE_COLOR: Color = [0.333, 1.0, 0.333, 1.0];
/// The yellow of the Microsoft badge (`§e`).
const MICROSOFT_COLOR: Color = [1.0, 1.0, 0.333, 1.0];
/// A row under the pointer.
const HOVER_SHADE: Color = [1.0, 1.0, 1.0, 0.08];

/// The kind of an account, shown as a coloured badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountBadge {
    /// A free account with a chosen name and no login.
    Offline,
    /// A Microsoft account.
    Microsoft,
}

impl AccountBadge {
    /// The translation key of the badge text.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Offline => "umicraft.accounts.badge.offline",
            Self::Microsoft => "umicraft.accounts.badge.microsoft",
        }
    }

    /// The badge colour.
    #[must_use]
    pub fn color(self) -> Color {
        match self {
            Self::Offline => OFFLINE_COLOR,
            Self::Microsoft => MICROSOFT_COLOR,
        }
    }
}

/// What the screen needs to know about one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    /// The account's player name.
    pub username: String,
    /// The account kind.
    pub badge: AccountBadge,
    /// The 8x8 RGBA face of the player's skin (256 bytes), or
    /// `None` for a flat placeholder while it is not known.
    pub face: Option<Arc<[u8]>>,
}

/// The decorations of a row.
#[derive(Debug, Clone, Copy)]
pub(super) struct RowFlags {
    /// The row is the selected one (outline box).
    pub selected: bool,
    /// The pointer is over the row.
    pub hovered: bool,
}

/// Draws `row` with its top-left corner at GUI `at`.
pub(super) fn draw_row(
    ui: &mut Ui,
    (gui, atlas, lang): (&Gui, &FontAtlas, &Lang),
    row: &AccountRow,
    at: [f32; 2],
    flags: RowFlags,
) {
    let [x, y] = at;
    if flags.selected {
        // The outline of `GuiSlot.drawSelectionBox`.
        let outer = gui.rect(x - 2.0, y - 2.0, ROW_WIDTH + 4.0, ROW_HEIGHT + 4.0);
        ui.draw().rect(outer, DIM_GRAY);
        let inner = gui.rect(x - 1.0, y - 1.0, ROW_WIDTH + 2.0, ROW_HEIGHT + 2.0);
        ui.draw().rect(inner, [0.0, 0.0, 0.0, 1.0]);
    }
    if flags.hovered {
        let shade = gui.rect(x, y, ROW_WIDTH, ROW_HEIGHT);
        ui.draw().rect(shade, HOVER_SHADE);
    }
    draw_face(ui, gui, row.face.as_deref(), [x, y], ROW_HEIGHT);
    let text_x = x + ROW_HEIGHT + 3.0;
    let name = TextPlacement::left([text_x, y + 5.0], WHITE);
    gui.text(ui, atlas, &row.username, name);
    let badge = TextPlacement::left([text_x, y + 18.0], row.badge.color());
    gui.text(ui, atlas, lang.tr(row.badge.key()), badge);
}
