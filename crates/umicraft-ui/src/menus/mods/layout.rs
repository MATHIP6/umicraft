// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the parts of the panel pages of the mods menu sit: one panel centred
//! on the screen, a sidebar of categories on its left and the page on its right
//! (a header, a search row and the grid of cards, or the settings of one mod).
//! Everything is computed in GUI units, half the logical pixels of the 1280 x
//! 720 reference of the design, and returned in physical pixels.

use crate::geom::Rect;
use crate::menus::common::Gui;

/// Width of the panel, in GUI units, when the screen is wide enough.
const PANEL_WIDTH: f32 = 500.0;
/// Height of the panel when the screen is tall enough.
const PANEL_HEIGHT: f32 = 295.0;
/// Room kept free under the panel for the hotbar and a gap above it, so the
/// panel never covers the hotbar.
const HOTBAR_ROOM: f32 = 31.0;
/// Space kept free between the panel and the other screen edges.
const SCREEN_MARGIN: f32 = 8.0;
/// Width of the sidebar.
pub(super) const SIDEBAR_WIDTH: f32 = 108.0;
/// Space between the sidebar edge and its content.
pub(super) const SIDEBAR_PAD: f32 = 7.0;
/// Space between the sidebar and the page, and between the page and the right
/// edge of the panel.
const PAGE_PAD_X: f32 = 12.0;
/// Space above the header and under the footer.
const PAGE_PAD_Y: f32 = 10.0;
/// Space between two rows of the page.
pub(super) const GAP: f32 = 8.0;
/// Space between two cards, in a row and from one row to the next.
const CARD_GAP: f32 = 6.0;
/// Height of the header band, and the edge of the square close cross.
const HEADER_HEIGHT: f32 = 17.0;
/// Height of the search row.
const ROW_HEIGHT: f32 = 20.0;
/// Width of the All / Enabled segmented control.
const FILTER_WIDTH: f32 = 100.0;
/// Size of the back button of the settings page.
const BACK_WIDTH: f32 = 46.0;
const BACK_HEIGHT: f32 = 15.0;
/// Height of the band with the icon, the name and the switch of a mod.
const MOD_HEADER_HEIGHT: f32 = 23.0;
/// Height of the footer line.
const FOOTER_HEIGHT: f32 = 10.0;
/// Height of the brand row of the sidebar and of the label under it.
const BRAND_HEIGHT: f32 = 15.0;
const SECTION_LABEL_HEIGHT: f32 = 8.0;
/// Height of a sidebar item and the space between two.
pub(super) const ITEM_HEIGHT: f32 = 19.0;
pub(super) const ITEM_GAP: f32 = 2.0;
/// Height of the button that opens the HUD editor.
const EDIT_HEIGHT: f32 = 22.0;
/// Height of a mod card.
pub(super) const CARD_HEIGHT: f32 = 92.0;
/// List width, in GUI units, from which the cards go in three columns, and in
/// two; below, in one.
const THREE_COLUMNS_FROM: f32 = 280.0;
const TWO_COLUMNS_FROM: f32 = 180.0;

/// The rectangles of the panel pages (list and settings), in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModsLayout {
    /// The panel.
    pub panel: Rect,
    /// The sidebar on its left.
    pub sidebar: Rect,
    /// The logo and the name of the brand at the top of the sidebar.
    pub brand: Rect,
    /// The label over the categories.
    pub section: Rect,
    /// Where the first sidebar item is; the others follow, see [`item_rect`].
    pub items: Rect,
    /// The button of the sidebar that opens the HUD editor.
    pub edit: Rect,
    /// The header band of the page: the title, or the back button.
    pub header: Rect,
    /// The cross at the right end of the header: back to the landing page.
    pub close: Rect,
    /// The search box of the list page.
    pub search: Rect,
    /// The All / Enabled control, right of the search box.
    pub filter: Rect,
    /// The back button of the settings page, at the left of the header.
    pub back: Rect,
    /// The band of the settings page with the icon, the name and the switch.
    pub mod_header: Rect,
    /// The area of the card grid.
    pub list: Rect,
    /// The area of the settings of a mod, under its band.
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
    let height = PANEL_HEIGHT.min(gui.height - SCREEN_MARGIN - HOTBAR_ROOM);
    let left = ((gui.width - width) * 0.5).floor();
    let top = (((gui.height - HOTBAR_ROOM - height) * 0.5).max(SCREEN_MARGIN)).floor();
    let sidebar_width = SIDEBAR_WIDTH.min(width * 0.4);

    let side_x = left + SIDEBAR_PAD;
    let side_inner = sidebar_width - 2.0 * SIDEBAR_PAD;
    let brand_y = top + PAGE_PAD_Y;
    let section_y = brand_y + BRAND_HEIGHT + 8.0;
    let items_y = section_y + SECTION_LABEL_HEIGHT + 4.0;

    let x = left + sidebar_width + PAGE_PAD_X;
    let inner = width - sidebar_width - 2.0 * PAGE_PAD_X;
    let header_y = top + PAGE_PAD_Y;
    let row_y = header_y + HEADER_HEIGHT + GAP;
    let bottom = top + height - PAGE_PAD_Y - FOOTER_HEIGHT;
    let list_y = row_y + ROW_HEIGHT + GAP;
    let mod_y = header_y + HEADER_HEIGHT + GAP;
    let detail_y = mod_y + MOD_HEADER_HEIGHT + GAP;
    let filter_width = FILTER_WIDTH.min(inner * 0.4);
    let hint = gui.rect(
        x + inner * 0.5,
        bottom + FOOTER_HEIGHT * 0.5 + 2.0,
        0.0,
        0.0,
    );
    ModsLayout {
        panel: gui.rect(left, top, width, height),
        sidebar: gui.rect(left, top, sidebar_width, height),
        brand: gui.rect(side_x, brand_y, side_inner, BRAND_HEIGHT),
        section: gui.rect(side_x + 3.0, section_y, side_inner, SECTION_LABEL_HEIGHT),
        items: gui.rect(side_x, items_y, side_inner, ITEM_HEIGHT),
        edit: gui.rect(
            side_x,
            top + height - PAGE_PAD_Y - EDIT_HEIGHT,
            side_inner,
            EDIT_HEIGHT,
        ),
        header: gui.rect(x, header_y, inner, HEADER_HEIGHT),
        close: gui.rect(
            x + inner - HEADER_HEIGHT,
            header_y,
            HEADER_HEIGHT,
            HEADER_HEIGHT,
        ),
        search: gui.rect(x, row_y, inner - filter_width - 5.0, ROW_HEIGHT),
        filter: gui.rect(x + inner - filter_width, row_y, filter_width, ROW_HEIGHT),
        back: gui.rect(
            x,
            header_y + (HEADER_HEIGHT - BACK_HEIGHT) * 0.5,
            BACK_WIDTH,
            BACK_HEIGHT,
        ),
        mod_header: gui.rect(x, mod_y, inner, MOD_HEADER_HEIGHT),
        list: gui.rect(x, list_y, inner, bottom - list_y),
        detail: gui.rect(x, detail_y, inner, bottom - detail_y),
        hint: [hint.x, hint.y],
        scale: gui.scale,
    }
}

/// The rectangle of sidebar item number `index`, under the first one `items`.
#[must_use]
pub(super) fn item_rect(items: Rect, scale: f32, index: usize) -> Rect {
    Rect::new(
        items.x,
        items.y + index as f32 * (ITEM_HEIGHT + ITEM_GAP) * scale,
        items.width,
        items.height,
    )
}

/// How many columns of cards fit in an area `width_units` wide.
#[must_use]
pub(super) fn columns(width_units: f32) -> usize {
    if width_units >= THREE_COLUMNS_FROM {
        3
    } else if width_units >= TWO_COLUMNS_FROM {
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
        rows as f32 * (CARD_HEIGHT + CARD_GAP) - CARD_GAP
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
    let gap = CARD_GAP * scale;
    let width = (list.width - gap * (columns - 1) as f32) / columns as f32;
    let (row, column) = (index / columns, index % columns);
    Rect::new(
        list.x + column as f32 * (width + gap),
        list.y + (row as f32 * (CARD_HEIGHT + CARD_GAP) - offset) * scale,
        width,
        CARD_HEIGHT * scale,
    )
}
