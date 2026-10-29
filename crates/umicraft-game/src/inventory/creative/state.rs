// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`CreativeScreen`]: the state of the creative inventory window.
//!
//! The state holds only what vanilla keeps in `GuiContainerCreative`: the
//! selected tab, the scroll position and the search text. The items in the
//! player's slots and on the cursor stay in the
//! [`WindowManager`](crate::inventory::WindowManager), which every
//! transition receives; the click handlers live in [`click`](super::click).

use umicraft_protocol::packets::play::ItemStack;

use super::items::tab_items;
use super::scroll::{
    needs_scrollbar, Scroll, COLUMNS, GRID_CELLS, SCROLLBAR_HEIGHT, SCROLLBAR_WIDTH, SCROLLBAR_X,
    SCROLLBAR_Y,
};
use super::search::search_items;
use super::tabs::CreativeTab;
use super::target::{slot_at, CreativeSlot};
use crate::inventory::tooltip::TooltipText;

/// The longest search text (`searchField.setMaxStringLength(15)`).
const MAX_SEARCH_LENGTH: usize = 15;

/// Resolves unresolved tooltip text to the displayed string (see
/// [`search_items`]).
pub type Resolver<'a> = &'a dyn Fn(&TooltipText) -> String;

/// The creative inventory window.
///
/// Keep one for the whole session: vanilla remembers the selected tab
/// between openings (`selectedTabIndex` is static). Call
/// [`reopen`](Self::reopen) each time the window opens.
#[derive(Debug, Clone, PartialEq)]
pub struct CreativeScreen {
    pub(super) tab: CreativeTab,
    pub(super) scroll: Scroll,
    search: String,
    results: Vec<ItemStack>,
    /// `field_147057_D`: set by every click, makes the next key press in the
    /// search tab start a new query.
    pub(super) restart_search_on_key: bool,
}

impl Default for CreativeScreen {
    fn default() -> Self {
        Self {
            tab: CreativeTab::BuildingBlocks,
            scroll: Scroll::new(),
            search: String::new(),
            results: Vec::new(),
            restart_search_on_key: false,
        }
    }
}

impl CreativeScreen {
    /// A screen on the Building Blocks tab.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The selected tab.
    #[must_use]
    pub fn tab(&self) -> CreativeTab {
        self.tab
    }

    /// Re-selects the remembered tab when the window opens (`initGui`):
    /// the scroll position and the search text start over.
    pub fn reopen(&mut self, resolve: Resolver<'_>) {
        self.select_tab(self.tab, resolve);
    }

    /// Selects a tab (`setCurrentCreativeTab`): the scroll position resets,
    /// and the search tab starts with an empty query that lists every item.
    ///
    /// Vanilla does this when the mouse is *released* over a tab button
    /// ([`CreativeTab::at`]); selecting the current tab again resets it too.
    pub fn select_tab(&mut self, tab: CreativeTab, resolve: Resolver<'_>) {
        self.tab = tab;
        self.scroll.reset();
        if tab.shows_search_box() {
            self.search.clear();
            self.refresh_search(resolve);
        }
    }

    /// Selects the search tab unless it is already open: what the chat key
    /// does in an item tab (`keyTyped`).
    pub fn open_search(&mut self, resolve: Resolver<'_>) {
        if self.tab != CreativeTab::Search {
            self.select_tab(CreativeTab::Search, resolve);
        }
    }

    /// Everything the selected tab lists, before scrolling: the tab's items,
    /// the search results, or nothing for the inventory tab.
    #[must_use]
    pub fn items(&self) -> &[ItemStack] {
        match self.tab {
            CreativeTab::Search => &self.results,
            CreativeTab::Inventory => &[],
            tab => tab_items(tab),
        }
    }

    /// The 45 visible grid cells, row by row, for the current scroll
    /// position (`ContainerCreative.scrollTo`).
    #[must_use]
    pub fn grid(&self) -> [Option<&ItemStack>; GRID_CELLS] {
        let items = self.items();
        let first = self.scroll.first_row(items.len()) * COLUMNS;
        std::array::from_fn(|cell| items.get(first + cell))
    }

    /// The stack in a grid cell (0 to 44), `None` for an empty cell.
    #[must_use]
    pub(super) fn grid_stack(&self, cell: usize) -> Option<&ItemStack> {
        self.grid().get(cell).copied().flatten()
    }

    /// The slot under a point relative to the window's top-left corner.
    #[must_use]
    pub fn slot_at(&self, x: i32, y: i32) -> Option<CreativeSlot> {
        slot_at(self.tab, x, y)
    }

    /// The scroll position and drag state, for drawing the handle.
    #[must_use]
    pub fn scroll(&self) -> &Scroll {
        &self.scroll
    }

    /// Whether the scrollbar is usable: an item tab with more than one page
    /// (`needsScrollBars`).
    #[must_use]
    pub fn needs_scrollbar(&self) -> bool {
        self.tab.has_scrollbar() && needs_scrollbar(self.items().len())
    }

    /// Applies a mouse wheel movement; `delta` is positive when scrolling up.
    pub fn wheel(&mut self, delta: i32) {
        if self.tab.has_scrollbar() {
            self.scroll.wheel(delta, self.items().len());
        }
    }

    /// A left press at a point relative to the window: starts a scrollbar
    /// drag when it lands on the track. Returns whether it did.
    pub fn press_scrollbar(&mut self, x: i32, y: i32) -> bool {
        let on_track = (SCROLLBAR_X..SCROLLBAR_X + SCROLLBAR_WIDTH).contains(&x)
            && (SCROLLBAR_Y..SCROLLBAR_Y + SCROLLBAR_HEIGHT).contains(&y);
        if on_track {
            let items = if self.tab.has_scrollbar() {
                self.items().len()
            } else {
                0
            };
            self.scroll.begin_drag(items);
        }
        on_track
    }

    /// Moves the dragged handle to the mouse height `y` (relative to the
    /// window); does nothing when no drag is in progress.
    pub fn drag_scrollbar(&mut self, y: i32) {
        self.scroll.drag_to((y - SCROLLBAR_Y) as f32);
    }

    /// Ends a scrollbar drag (mouse button released).
    pub fn release_scrollbar(&mut self) {
        self.scroll.end_drag();
    }

    /// The text in the search box.
    #[must_use]
    pub fn search_text(&self) -> &str {
        &self.search
    }

    /// Types a character into the search box; returns whether it was
    /// consumed (only on the search tab, only printable characters, at most
    /// 15 of them). The results refresh at once.
    pub fn search_input(&mut self, ch: char, resolve: Resolver<'_>) -> bool {
        if !self.tab.shows_search_box() {
            return false;
        }
        self.restart_search_if_pending();
        let allowed = ch != '\u{a7}' && ch >= ' ' && ch != '\u{7f}';
        if allowed && self.search.chars().count() < MAX_SEARCH_LENGTH {
            self.search.push(ch);
            self.refresh_search(resolve);
        }
        allowed
    }

    /// Deletes the last character of the search text; returns whether the
    /// key was consumed (it is on the search tab, even when the text is
    /// empty).
    pub fn search_backspace(&mut self, resolve: Resolver<'_>) -> bool {
        if !self.tab.shows_search_box() {
            return false;
        }
        self.restart_search_if_pending();
        self.search.pop();
        self.refresh_search(resolve);
        true
    }

    /// Clears a query left over from before the last click
    /// (`field_147057_D` in `keyTyped`), without refreshing the list.
    pub(super) fn restart_search_if_pending(&mut self) {
        if std::mem::take(&mut self.restart_search_on_key) {
            self.search.clear();
        }
    }

    /// `updateCreativeSearch`: refilters the catalogue and scrolls to the top.
    fn refresh_search(&mut self, resolve: Resolver<'_>) {
        self.results = search_items(&self.search, resolve);
        self.scroll.reset();
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Resolver showing language keys, enough for matching names.
    pub fn key_resolver(text: &TooltipText) -> String {
        match text {
            TooltipText::Literal(text) => text.clone(),
            TooltipText::Translation { key, .. } => key.clone(),
            TooltipText::Concat(parts) => parts.iter().map(key_resolver).collect(),
        }
    }

    #[test]
    fn a_new_screen_shows_the_first_page_of_building_blocks() {
        let screen = CreativeScreen::new();
        assert_eq!(screen.tab(), CreativeTab::BuildingBlocks);
        let grid = screen.grid();
        assert_eq!(grid[0].map(|s| (s.item_id, s.damage)), Some((1, 0)));
        assert_eq!(
            grid[44].map(|s| s.item_id),
            Some(tab_items(CreativeTab::BuildingBlocks)[44].item_id)
        );
    }

    #[test]
    fn scrolling_shifts_the_grid_by_whole_rows() {
        let mut screen = CreativeScreen::new();
        let all = tab_items(CreativeTab::BuildingBlocks);
        screen.wheel(-1);
        // 158 items: 18 rows, 13 hidden, one wheel step = 1/12 of the track -> row 1.
        assert_eq!(screen.grid()[0], Some(&all[9]));
        screen.wheel(1);
        assert_eq!(screen.grid()[0], Some(&all[0]));
    }

    #[test]
    fn the_last_page_leaves_trailing_cells_empty() {
        let mut screen = CreativeScreen::new();
        screen.select_tab(CreativeTab::Redstone, &key_resolver);
        // 37 items fit on one page: 4 rows, the rest empty.
        let grid = screen.grid();
        assert_eq!(grid.iter().flatten().count(), 37);
        assert!(grid[37].is_none());
        assert!(!screen.needs_scrollbar());
    }

    #[test]
    fn selecting_a_tab_resets_the_scroll() {
        let mut screen = CreativeScreen::new();
        screen.wheel(-5);
        screen.select_tab(CreativeTab::Food, &key_resolver);
        assert_eq!(screen.scroll().position(), 0.0);
        assert_eq!(screen.items(), tab_items(CreativeTab::Food));
    }

    #[test]
    fn the_inventory_tab_lists_nothing_and_has_no_scrollbar() {
        let mut screen = CreativeScreen::new();
        screen.select_tab(CreativeTab::Inventory, &key_resolver);
        assert!(screen.items().is_empty());
        assert!(!screen.needs_scrollbar());
        screen.press_scrollbar(180, 30);
        assert!(!screen.scroll().is_dragging());
    }

    #[test]
    fn the_search_tab_lists_everything_until_text_is_typed() {
        let mut screen = CreativeScreen::new();
        screen.select_tab(CreativeTab::Search, &key_resolver);
        let everything = tab_items(CreativeTab::Search).len();
        assert_eq!(screen.items().len(), everything);
        for ch in "pickaxe".chars() {
            assert!(screen.search_input(ch, &key_resolver));
        }
        assert_eq!(screen.search_text(), "pickaxe");
        let found = screen.items();
        assert!(!found.is_empty() && found.len() < everything);
        assert!(found.iter().any(|s| s.item_id == 257)); // iron pickaxe
        assert!(found.iter().all(|s| s.item_id != 276));
    }

    #[test]
    fn typing_is_ignored_outside_the_search_tab_and_for_control_characters() {
        let mut screen = CreativeScreen::new();
        assert!(!screen.search_input('a', &key_resolver));
        assert!(!screen.search_backspace(&key_resolver));
        screen.select_tab(CreativeTab::Search, &key_resolver);
        assert!(!screen.search_input('\u{7f}', &key_resolver));
        assert!(!screen.search_input('\u{a7}', &key_resolver));
        assert_eq!(screen.search_text(), "");
    }

    #[test]
    fn the_search_text_is_limited_to_fifteen_characters_and_backspace_edits_it() {
        let mut screen = CreativeScreen::new();
        screen.select_tab(CreativeTab::Search, &key_resolver);
        for _ in 0..20 {
            screen.search_input('a', &key_resolver);
        }
        assert_eq!(screen.search_text().len(), 15);
        assert!(screen.search_backspace(&key_resolver));
        assert_eq!(screen.search_text().len(), 14);
    }

    #[test]
    fn searching_resets_the_scroll_and_reopening_clears_the_query() {
        let mut screen = CreativeScreen::new();
        screen.select_tab(CreativeTab::Search, &key_resolver);
        screen.wheel(-3);
        assert!(screen.scroll().position() > 0.0);
        screen.search_input('s', &key_resolver);
        assert_eq!(screen.scroll().position(), 0.0);
        screen.reopen(&key_resolver);
        assert_eq!(screen.search_text(), "");
        assert_eq!(screen.tab(), CreativeTab::Search);
    }

    #[test]
    fn open_search_switches_once() {
        let mut screen = CreativeScreen::new();
        screen.open_search(&key_resolver);
        screen.search_input('x', &key_resolver);
        screen.open_search(&key_resolver);
        assert_eq!(screen.search_text(), "x");
    }

    #[test]
    fn the_scrollbar_drag_follows_the_mouse_on_a_long_list() {
        let mut screen = CreativeScreen::new();
        assert!(!screen.press_scrollbar(10, 30));
        assert!(screen.press_scrollbar(180, 30));
        screen.drag_scrollbar(18 + 7 + 97);
        assert!(screen.scroll().position() > 0.9);
        screen.release_scrollbar();
        screen.drag_scrollbar(18);
        assert!(screen.scroll().position() > 0.9);
    }
}
