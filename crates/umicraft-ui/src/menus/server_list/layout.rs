// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rectangles of the server list screen.

use crate::geom::Rect;
use crate::menus::common::Gui;

use super::LIST_TOP;

/// The rectangles of the server list screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerListLayout {
    /// The scrolling list area.
    pub list: Rect,
    /// The account chooser button at the top right (a Umicraft addition).
    pub accounts: Rect,
    /// Join Server.
    pub join: Rect,
    /// Direct Connect.
    pub direct: Rect,
    /// Add Server.
    pub add: Rect,
    /// Edit.
    pub edit: Rect,
    /// Delete.
    pub delete: Rect,
    /// Refresh.
    pub refresh: Rect,
    /// Cancel.
    pub cancel: Rect,
}

/// Computes the vanilla layout for `viewport`, plus the account button.
#[must_use]
pub fn server_list_layout(viewport: Rect) -> ServerListLayout {
    let gui = Gui::new(viewport);
    let (cx, h) = (gui.cx(), gui.height);
    ServerListLayout {
        list: gui.rect(0.0, LIST_TOP, gui.width, h - 64.0 - LIST_TOP),
        accounts: gui.rect(gui.width - 126.0, 6.0, 120.0, 20.0),
        join: gui.rect(cx - 154.0, h - 52.0, 100.0, 20.0),
        direct: gui.rect(cx - 50.0, h - 52.0, 100.0, 20.0),
        add: gui.rect(cx + 54.0, h - 52.0, 100.0, 20.0),
        edit: gui.rect(cx - 154.0, h - 28.0, 70.0, 20.0),
        delete: gui.rect(cx - 74.0, h - 28.0, 70.0, 20.0),
        refresh: gui.rect(cx + 4.0, h - 28.0, 70.0, 20.0),
        cancel: gui.rect(cx + 80.0, h - 28.0, 75.0, 20.0),
    }
}
