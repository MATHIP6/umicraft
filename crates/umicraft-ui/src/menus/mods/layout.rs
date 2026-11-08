// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the parts of the panel pages of the mods menu sit: one panel centred on the screen with
//! a header, a search row, the tabs and the list below, and a hint line.
//! Everything is computed in GUI units and returned in physical pixels.

use crate::geom::Rect;
use crate::menus::common::Gui;

/// Width of the panel, in GUI units, when the screen is wide enough.
const PANEL_WIDTH: f32 = 320.0;
/// Height of the panel when the screen is tall enough.
const PANEL_HEIGHT: f32 = 200.0;
/// Space kept free between the panel and the screen edge.
const SCREEN_MARGIN: f32 = 8.0;
/// Space between the panel edge and its content.
const PAD: f32 = 8.0;
/// Space between two rows or two neighbouring widgets.
pub(super) const GAP: f32 = 4.0;
/// Height of the title band.
const HEADER_HEIGHT: f32 = 12.0;
/// Edge of the square close cross.
const CLOSE_SIZE: f32 = 12.0;
/// Height of the search row.
const ROW_HEIGHT: f32 = 14.0;
/// Width of the "back" button of the settings page.
const BACK_WIDTH: f32 = 54.0;
/// Height of the tab row.
const TABS_HEIGHT: f32 = 12.0;
/// Height of the hint line at the bottom.
const HINT_HEIGHT: f32 = 8.0;
/// Height of a mod card.
pub(super) const CARD_HEIGHT: f32 = 34.0;
/// Panel width above which the cards go in two columns.
const TWO_COLUMNS_FROM: f32 = 240.0;

/// The rectangles of the panel pages (list and settings), in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModsLayout {
    /// The panel.
    pub panel: Rect,
    /// The title band.
    pub header: Rect,
    /// The cross at the right end of the header: back to the landing page.
    pub close: Rect,
    /// The search box of the list page.
    pub search: Rect,
    /// The "back" button of the settings page, left of the search row.
    pub back: Rect,
    /// The tab row.
    pub tabs: Rect,
    /// The area of the card list.
    pub list: Rect,
    /// The area of the settings page: the list area plus the tab row.
    pub detail: Rect,
    /// Where the hint is centred.
    pub hint: [f32; 2],
    /// Physical pixels per GUI unit.
    pub scale: f32,
}

/// Computes the layout for `viewport`.
#[must_use]
pub fn mods_layout(viewport: Rect) -> ModsLayout {
    let gui = Gui::new(viewport);
    let width = PANEL_WIDTH.min(gui.width - 2.0 * SCREEN_MARGIN);
    let height = PANEL_HEIGHT.min(gui.height - 2.0 * SCREEN_MARGIN);
    let left = ((gui.width - width) * 0.5).floor();
    let top = ((gui.height - height) * 0.5).floor();
    let inner = width - 2.0 * PAD;
    let (x, header_y) = (left + PAD, top + PAD);
    let row_y = header_y + HEADER_HEIGHT + GAP;
    let tabs_y = row_y + ROW_HEIGHT + GAP;
    let list_y = tabs_y + TABS_HEIGHT + GAP;
    let bottom = top + height - PAD - HINT_HEIGHT;
    let detail_y = tabs_y;
    ModsLayout {
        panel: gui.rect(left, top, width, height),
        header: gui.rect(x, header_y, inner, HEADER_HEIGHT),
        close: gui.rect(x + inner - CLOSE_SIZE, header_y, CLOSE_SIZE, HEADER_HEIGHT),
        search: gui.rect(x, row_y, inner, ROW_HEIGHT),
        back: gui.rect(x, row_y, BACK_WIDTH, ROW_HEIGHT),
        tabs: gui.rect(x, tabs_y, inner, TABS_HEIGHT),
        list: gui.rect(x, list_y, inner, bottom - list_y),
        detail: gui.rect(x, detail_y, inner, bottom - detail_y),
        hint: {
            let hint = gui.rect(
                left + width * 0.5,
                bottom + HINT_HEIGHT * 0.5 + 1.0,
                0.0,
                0.0,
            );
            [hint.x, hint.y]
        },
        scale: gui.scale,
    }
}

/// How many columns of cards fit in an area `width_units` wide.
#[must_use]
pub(super) fn columns(width_units: f32) -> usize {
    if width_units >= TWO_COLUMNS_FROM {
        2
    } else {
        1
    }
}

/// The height in GUI units of `count` cards in `columns` columns.
#[must_use]
pub(super) fn content_height(count: usize, columns: usize) -> f32 {
    let rows = count.div_ceil(columns.max(1));
    if rows == 0 {
        0.0
    } else {
        rows as f32 * (CARD_HEIGHT + GAP) - GAP
    }
}

/// The rectangle of card number `index` in an area `list`, scrolled down by
/// `offset` GUI units.
#[must_use]
pub(super) fn card_rect(
    list: Rect,
    scale: f32,
    index: usize,
    (columns, offset): (usize, f32),
) -> Rect {
    let columns = columns.max(1);
    let gap = GAP * scale;
    let width = (list.width - gap * (columns - 1) as f32) / columns as f32;
    let (row, column) = (index / columns, index % columns);
    Rect::new(
        list.x + column as f32 * (width + gap),
        list.y + (row as f32 * (CARD_HEIGHT + GAP) - offset) * scale,
        width,
        CARD_HEIGHT * scale,
    )
}
