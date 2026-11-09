// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure layout of the account screens, in physical pixels.
//!
//! GUI units, `cx = width / 2`: the list runs from `y = 32` to `height - 64`
//! like the server list; rows are 36 units apart, 300 wide from `cx - 150`,
//! the first one 4 below the top; the scrollbar sits at `cx + 154`. Two button
//! rows at `height - 52` (Play 150, Add 150) and `height - 28` (Edit 70,
//! Delete 70, Refresh 70, Cancel 75) mirror the server list so the screens feel
//! alike. The name dialog stacks a 200x20 field and 200x20 buttons 24 apart.

use crate::geom::Rect;
use crate::menus::common::Gui;

/// The top of the list in GUI units.
pub(super) const LIST_TOP: f32 = 32.0;
/// Space above the first row inside the list.
pub(super) const LIST_PADDING: f32 = 4.0;
/// The distance between two rows.
pub(super) const SLOT_HEIGHT: f32 = 36.0;
/// The height of a row (the head is a square of this size).
pub(super) const ROW_HEIGHT: f32 = 32.0;
/// The width of a row.
pub(super) const ROW_WIDTH: f32 = 300.0;

/// The rectangles of the account list screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccountsLayout {
    /// The scrolling list area.
    pub list: Rect,
    /// Play with the selected account.
    pub play: Rect,
    /// Add account.
    pub add: Rect,
    /// Edit (offline accounts only).
    pub edit: Rect,
    /// Delete.
    pub delete: Rect,
    /// Refresh the sessions.
    pub refresh: Rect,
    /// Cancel.
    pub cancel: Rect,
}

/// Computes the list screen layout for `viewport`.
#[must_use]
pub fn accounts_layout(viewport: Rect) -> AccountsLayout {
    let gui = Gui::new(viewport);
    let (cx, h) = (gui.cx(), gui.height);
    AccountsLayout {
        list: gui.rect(0.0, LIST_TOP, gui.width, h - 64.0 - LIST_TOP),
        play: gui.rect(cx - 154.0, h - 52.0, 150.0, 20.0),
        add: gui.rect(cx + 4.0, h - 52.0, 150.0, 20.0),
        edit: gui.rect(cx - 154.0, h - 28.0, 70.0, 20.0),
        delete: gui.rect(cx - 74.0, h - 28.0, 70.0, 20.0),
        refresh: gui.rect(cx + 4.0, h - 28.0, 70.0, 20.0),
        cancel: gui.rect(cx + 80.0, h - 28.0, 75.0, 20.0),
    }
}

/// The rectangle of row `index` for a scroll `offset` in GUI units.
#[must_use]
pub fn account_row_rect(viewport: Rect, index: usize, offset: f32) -> Rect {
    let gui = Gui::new(viewport);
    let y = LIST_TOP + LIST_PADDING + index as f32 * SLOT_HEIGHT - offset;
    gui.rect(gui.cx() - ROW_WIDTH / 2.0, y, ROW_WIDTH, ROW_HEIGHT)
}

/// The row under the physical position `point`, or `None` between rows.
#[must_use]
pub fn account_row_at(viewport: Rect, point: [f32; 2], offset: f32) -> Option<usize> {
    let gui = Gui::new(viewport);
    let [x, y] = gui.to_gui(point);
    let inside_x = (x - gui.cx()).abs() <= ROW_WIDTH / 2.0;
    let relative = y - LIST_TOP - LIST_PADDING + offset;
    let index = (relative / SLOT_HEIGHT).floor();
    let in_row = relative >= 0.0 && relative - index * SLOT_HEIGHT < ROW_HEIGHT;
    (inside_x && in_row).then_some(index as usize)
}

/// The scroll offset that brings row `index` fully into a list `view` units
/// high, starting from `offset` (GUI units).
#[must_use]
pub fn offset_to_show(index: usize, offset: f32, view: f32) -> f32 {
    let top = LIST_PADDING + index as f32 * SLOT_HEIGHT;
    let bottom = top + ROW_HEIGHT + LIST_PADDING;
    if top < offset {
        top - LIST_PADDING
    } else if bottom > offset + view {
        bottom - view
    } else {
        offset
    }
}

/// The rectangles of the Add and Edit dialogs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NameDialogLayout {
    /// The username field.
    pub field: Rect,
    /// The confirming button (Offline / Save).
    pub primary: Rect,
    /// The Microsoft button (a zero rectangle in the Edit dialog).
    pub microsoft: Rect,
    /// Cancel.
    pub cancel: Rect,
}

/// Computes the name dialog layout; `with_microsoft` is true for Add.
#[must_use]
pub fn name_dialog_layout(viewport: Rect, with_microsoft: bool) -> NameDialogLayout {
    let gui = Gui::new(viewport);
    let (x, quarter) = (gui.cx() - 100.0, gui.h_div(4.0));
    let button = |row: f32| gui.rect(x, quarter + 84.0 + row * 24.0, 200.0, 20.0);
    NameDialogLayout {
        field: gui.rect(x, 76.0, 200.0, 20.0),
        primary: button(0.0),
        microsoft: if with_microsoft {
            button(1.0)
        } else {
            Rect::ZERO
        },
        cancel: button(if with_microsoft { 2.0 } else { 1.0 }),
    }
}
