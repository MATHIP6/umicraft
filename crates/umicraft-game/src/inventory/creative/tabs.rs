// SPDX-License-Identifier: MIT OR Apache-2.0

//! The twelve creative tabs (MCP `CreativeTabs`) and their tab-strip geometry.

/// Width of the creative window (`GuiContainerCreative.xSize`).
pub const WINDOW_WIDTH: i32 = 195;
/// Height of the creative window (`GuiContainerCreative.ySize`).
pub const WINDOW_HEIGHT: i32 = 136;
/// Width of one tab button.
pub const TAB_WIDTH: i32 = 28;
/// Height of one tab button.
pub const TAB_HEIGHT: i32 = 32;

/// A creative tab, in `CreativeTabs.creativeTabArray` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreativeTab {
    /// Building Blocks (`tabBlock`).
    BuildingBlocks,
    /// Decoration Blocks (`tabDecorations`).
    Decorations,
    /// Redstone (`tabRedstone`).
    Redstone,
    /// Transportation (`tabTransport`).
    Transportation,
    /// Miscellaneous (`tabMisc`).
    Misc,
    /// Search Items (`tabAllSearch`): every item, filtered by the search box.
    Search,
    /// Foodstuffs (`tabFood`).
    Food,
    /// Tools (`tabTools`).
    Tools,
    /// Combat (`tabCombat`).
    Combat,
    /// Brewing (`tabBrewing`).
    Brewing,
    /// Materials (`tabMaterials`).
    Materials,
    /// Survival Inventory (`tabInventory`): the player's own slots.
    Inventory,
}

impl CreativeTab {
    /// Every tab in tab-index order.
    pub const ALL: [Self; 12] = [
        Self::BuildingBlocks,
        Self::Decorations,
        Self::Redstone,
        Self::Transportation,
        Self::Misc,
        Self::Search,
        Self::Food,
        Self::Tools,
        Self::Combat,
        Self::Brewing,
        Self::Materials,
        Self::Inventory,
    ];

    /// `CreativeTabs.getTabIndex`, 0 to 11.
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    /// The tab with this index, `None` out of range.
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// `CreativeTabs.getTabLabel`, the suffix of the language key.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::BuildingBlocks => "buildingBlocks",
            Self::Decorations => "decorations",
            Self::Redstone => "redstone",
            Self::Transportation => "transportation",
            Self::Misc => "misc",
            Self::Search => "search",
            Self::Food => "food",
            Self::Tools => "tools",
            Self::Combat => "combat",
            Self::Brewing => "brewing",
            Self::Materials => "materials",
            Self::Inventory => "inventory",
        }
    }

    /// `CreativeTabs.getTranslatedTabLabel`: the language key of the title
    /// and of the hover text (`itemGroup.<label>`).
    #[must_use]
    pub fn translation_key(self) -> String {
        format!("itemGroup.{}", self.label())
    }

    /// The `(item id, damage)` drawn on the tab button
    /// (`getTabIconItem`, `getIconItemDamage`).
    #[must_use]
    pub fn icon(self) -> (i16, i16) {
        match self {
            Self::BuildingBlocks => (45, 0), // brick_block
            Self::Decorations => (175, 5),   // double_plant, PAEONIA
            Self::Redstone => (331, 0),      // redstone
            Self::Transportation => (27, 0), // golden_rail
            Self::Misc => (327, 0),          // lava_bucket
            Self::Search => (345, 0),        // compass
            Self::Food => (260, 0),          // apple
            Self::Tools => (258, 0),         // iron_axe
            Self::Combat => (283, 0),        // golden_sword
            Self::Brewing => (373, 0),       // potionitem
            Self::Materials => (280, 0),     // stick
            Self::Inventory => (54, 0),      // chest
        }
    }

    /// Whether the tab scrolls its item list (`!setNoScrollbar`).
    #[must_use]
    pub fn has_scrollbar(self) -> bool {
        self != Self::Inventory
    }

    /// Whether the search box is shown.
    #[must_use]
    pub fn shows_search_box(self) -> bool {
        self == Self::Search
    }

    /// Whether the tab shows the survival inventory instead of the item grid.
    #[must_use]
    pub fn shows_player_inventory(self) -> bool {
        self == Self::Inventory
    }

    /// Whether the title is drawn in the window (`drawInForegroundOfTab`).
    #[must_use]
    pub fn shows_title(self) -> bool {
        self != Self::Inventory
    }

    /// The name of the background texture below
    /// `textures/gui/container/creative_inventory/`, without extension
    /// (`"tab_" + getBackgroundImageName()`).
    #[must_use]
    pub fn background_texture(self) -> &'static str {
        match self {
            Self::Search => "tab_item_search",
            Self::Inventory => "tab_inventory",
            _ => "tab_items",
        }
    }

    /// `getTabColumn`: the column of the tab in its row (0 to 5).
    #[must_use]
    pub fn column(self) -> i32 {
        (self.index() % 6) as i32
    }

    /// `isTabInFirstRow`: true for the strip above the window.
    #[must_use]
    pub fn in_top_row(self) -> bool {
        self.index() < 6
    }

    /// The hit box of the tab button as `(x, y)` relative to the window's
    /// top-left corner; it is [`TAB_WIDTH`] by [`TAB_HEIGHT`]
    /// (`GuiContainerCreative.func_147049_a`).
    #[must_use]
    pub fn hit_origin(self) -> (i32, i32) {
        let column = self.column();
        let x = match column {
            0 => 0,
            5 => WINDOW_WIDTH - TAB_WIDTH + 2,
            _ => TAB_WIDTH * column + column,
        };
        let y = if self.in_top_row() {
            -32
        } else {
            WINDOW_HEIGHT
        };
        (x, y)
    }

    /// Where the button is drawn, relative to the window's top-left corner
    /// (`func_147051_a`); the texture is [`TAB_WIDTH`] by [`TAB_HEIGHT`].
    #[must_use]
    pub fn draw_origin(self) -> (i32, i32) {
        let column = self.column();
        let x = match column {
            5 => WINDOW_WIDTH - TAB_WIDTH,
            0 => 0,
            _ => TAB_WIDTH * column + column,
        };
        let y = if self.in_top_row() {
            -28
        } else {
            WINDOW_HEIGHT - 4
        };
        (x, y)
    }

    /// The top-left of the button's slice in `tabs.png`; `selected` picks the
    /// raised variant (`func_147051_a`: `u = column * 28`, `v` offset by 32
    /// when selected and by 64 for the bottom row).
    #[must_use]
    pub fn texture_origin(self, selected: bool) -> (i32, i32) {
        let u = self.column() * TAB_WIDTH;
        let v = if selected { TAB_HEIGHT } else { 0 } + if self.in_top_row() { 0 } else { 64 };
        (u, v)
    }

    /// The tab whose button contains the point, given relative to the
    /// window's top-left corner (`func_147049_a` over all tabs).
    #[must_use]
    pub fn at(x: i32, y: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| {
            let (left, top) = tab.hit_origin();
            (left..=left + TAB_WIDTH).contains(&x) && (top..=top + TAB_HEIGHT).contains(&y)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_round_trip_and_follow_the_reference_order() {
        for (index, tab) in CreativeTab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), index);
            assert_eq!(CreativeTab::from_index(index), Some(tab));
        }
        assert_eq!(CreativeTab::from_index(12), None);
        assert_eq!(CreativeTab::Search.index(), 5);
        assert_eq!(CreativeTab::Inventory.index(), 11);
    }

    #[test]
    fn language_keys_use_the_item_group_prefix() {
        assert_eq!(
            CreativeTab::BuildingBlocks.translation_key(),
            "itemGroup.buildingBlocks"
        );
        assert_eq!(CreativeTab::Search.translation_key(), "itemGroup.search");
    }

    #[test]
    fn only_the_inventory_tab_lacks_scrollbar_and_title() {
        for tab in CreativeTab::ALL {
            assert_eq!(tab.has_scrollbar(), tab != CreativeTab::Inventory);
            assert_eq!(tab.shows_title(), tab != CreativeTab::Inventory);
            assert_eq!(tab.shows_search_box(), tab == CreativeTab::Search);
        }
    }

    #[test]
    fn backgrounds_match_the_reference_texture_names() {
        assert_eq!(CreativeTab::Food.background_texture(), "tab_items");
        assert_eq!(CreativeTab::Search.background_texture(), "tab_item_search");
        assert_eq!(CreativeTab::Inventory.background_texture(), "tab_inventory");
    }

    #[test]
    fn tab_hit_boxes_match_the_reference_geometry() {
        assert_eq!(CreativeTab::BuildingBlocks.hit_origin(), (0, -32));
        assert_eq!(CreativeTab::Decorations.hit_origin(), (29, -32));
        assert_eq!(CreativeTab::Search.hit_origin(), (169, -32));
        assert_eq!(CreativeTab::Food.hit_origin(), (0, 136));
        assert_eq!(CreativeTab::Inventory.hit_origin(), (169, 136));
    }

    #[test]
    fn tab_at_finds_the_button_under_a_point() {
        assert_eq!(CreativeTab::at(10, -10), Some(CreativeTab::BuildingBlocks));
        assert_eq!(CreativeTab::at(40, 150), Some(CreativeTab::Tools));
        assert_eq!(CreativeTab::at(100, 50), None);
    }

    #[test]
    fn texture_slices_follow_the_row_and_selection() {
        assert_eq!(CreativeTab::Redstone.texture_origin(false), (56, 0));
        assert_eq!(CreativeTab::Redstone.texture_origin(true), (56, 32));
        assert_eq!(CreativeTab::Tools.texture_origin(false), (28, 64));
        assert_eq!(CreativeTab::Tools.texture_origin(true), (28, 96));
    }
}
