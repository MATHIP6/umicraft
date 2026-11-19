// SPDX-License-Identifier: MIT OR Apache-2.0

//! The snapshot of the creative window the game thread publishes.
//!
//! [`CreativeScreen`] lives on the game thread because it edits the player's
//! inventory. The render thread only needs what is drawn and what decides
//! where the pointer points, which [`CreativeView`] carries.

use umicraft_game::inventory::creative::{CreativeScreen, CreativeTab};
use umicraft_protocol::packets::play::ItemStack;

/// What the renderer needs of the creative window.
#[derive(Debug, Clone, PartialEq)]
pub struct CreativeView {
    /// The selected tab.
    pub tab: CreativeTab,
    /// The distance of the scrollbar handle from the top of the track, in
    /// scaled pixels (`Scroll::handle_offset`).
    pub handle_offset: i32,
    /// Whether the scrollbar is usable (`needsScrollBars`).
    pub needs_scrollbar: bool,
    /// The text in the search box.
    pub search: String,
    /// The 45 visible grid cells, row by row.
    pub grid: Vec<Option<ItemStack>>,
}

impl CreativeView {
    /// The snapshot of `screen`.
    #[must_use]
    pub fn of(screen: &CreativeScreen) -> Self {
        Self {
            tab: screen.tab(),
            handle_offset: screen.scroll().handle_offset(),
            needs_scrollbar: screen.needs_scrollbar(),
            search: screen.search_text().to_owned(),
            grid: screen
                .grid()
                .into_iter()
                .map(|cell| cell.cloned())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_screen_shows_a_full_building_blocks_grid() {
        let view = CreativeView::of(&CreativeScreen::new());
        assert_eq!(view.tab, CreativeTab::BuildingBlocks);
        assert_eq!(view.grid.len(), 45);
        assert!(view.grid[0].is_some());
        assert_eq!(view.handle_offset, 0);
        assert!(view.search.is_empty());
    }
}
