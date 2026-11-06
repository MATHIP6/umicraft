// SPDX-License-Identifier: MIT OR Apache-2.0

//! The statistics screen (MCP `GuiStats`): four tabs over `GuiSlot` lists.
//!
//! Until the server answers the request, the screen shows
//! `multiplayer.downloadingStats` with a moving `oooooo` marker. Then the
//! General, Blocks, Items and Mobs buttons (80 wide, 52 above the bottom)
//! pick the list and Done (150 wide, right of the centre) leaves. A tab with
//! no rows is disabled. The Blocks and Items tables sort by column.
//!
//! The screen holds the tab, the scroll offsets and the sorts; the caller
//! passes [`StatsData`] every frame and calls [`StatsScreen::refresh`] when
//! a new answer replaces it, so the tab, the scrolling and the sorting
//! survive an update.

mod list;
mod lists;
mod model;
mod objects;
#[cfg(test)]
mod tests;

use umicraft_core::lang::Lang;

use self::list::ListFrame;
use self::lists::{draw_general, draw_mobs, GENERAL_HEIGHT, MOB_HEIGHT};
pub use self::model::{MobRow, ObjectRow, StatCell, StatRow, StatsData};
use self::objects::{Sort, Table, TableView, SLOT_HEIGHT};
use super::background::draw_menu_background;
use super::common::{escape_pressed, Gui, ScrollbarDrag, TextPlacement, WHITE};
use crate::context::Ui;
use crate::geom::Rect;
use crate::text::FontAtlas;

/// `IProgressMeter.lanSearchStates`, the marker of the loading text.
const LOADING_MARKERS: [&str; 7] = [
    "oooooo", "Oooooo", "oOoooo", "ooOooo", "oooOoo", "ooooOo", "oooooO",
];
/// Ticks per marker (`getSystemTime() / 150`, 50 ms per tick).
const TICKS_PER_MARKER: u32 = 3;

/// The tab a list belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatsTab {
    /// `stat.generalButton`: the counters.
    #[default]
    General,
    /// `stat.blocksButton`: crafted, used and mined blocks.
    Blocks,
    /// `stat.itemsButton`: depleted, crafted and used items.
    Items,
    /// `stat.mobsButton`: kills and deaths per mob.
    Mobs,
}

impl StatsTab {
    const ALL: [Self; 4] = [Self::General, Self::Blocks, Self::Items, Self::Mobs];

    fn key(self) -> &'static str {
        match self {
            Self::General => "stat.generalButton",
            Self::Blocks => "stat.blocksButton",
            Self::Items => "stat.itemsButton",
            Self::Mobs => "stat.mobsButton",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn is_empty(self, data: &StatsData) -> bool {
        match self {
            Self::General => false,
            Self::Blocks => data.blocks.is_empty(),
            Self::Items => data.items.is_empty(),
            Self::Mobs => data.mobs.is_empty(),
        }
    }
}

/// What the player did on the statistics screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsOutcome {
    /// Leave the screen (Done or Escape).
    Done,
}

/// The Done button for `viewport`.
#[must_use]
pub fn stats_done_rect(viewport: Rect) -> Rect {
    let gui = Gui::new(viewport);
    gui.rect(gui.cx() + 4.0, gui.height - 28.0, 150.0, 20.0)
}

/// The button of `tab` for `viewport`.
#[must_use]
pub fn stats_tab_rect(viewport: Rect, tab: StatsTab) -> Rect {
    let gui = Gui::new(viewport);
    let x = gui.cx() - 160.0 + 80.0 * tab.index() as f32;
    gui.rect(x, gui.height - 52.0, 80.0, 20.0)
}

/// The tab, scroll and sort state of the statistics screen.
#[derive(Debug, Clone, Default)]
pub struct StatsScreen {
    tab: StatsTab,
    offsets: [f32; 4],
    sorts: [Sort; 2],
    orders: [Vec<usize>; 2],
    pressed: Option<usize>,
    bar: ScrollbarDrag,
    ticks: u32,
}

impl StatsScreen {
    /// Creates the screen on the General tab, scrolled to the top.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The tab being shown.
    #[must_use]
    pub fn tab(&self) -> StatsTab {
        self.tab
    }

    /// The scroll offset of `tab` in GUI units.
    #[must_use]
    pub fn offset(&self, tab: StatsTab) -> f32 {
        self.offsets[tab.index()]
    }

    /// Advances the loading marker by one game tick.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    /// Re-sorts the tables for `data` after the server sent new numbers.
    pub fn refresh(&mut self, data: &StatsData) {
        self.orders[0] = self.sorts[0].order(&data.blocks);
        self.orders[1] = self.sorts[1].order(&data.items);
    }

    /// Draws one frame and returns the player's choice. `loading` is true
    /// until the server answered the request.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        data: &StatsData,
        loading: bool,
    ) -> Option<StatsOutcome> {
        draw_menu_background(ui);
        let gui = Gui::new(ui.viewport());
        if loading {
            self.draw_loading(ui, atlas, lang, &gui);
            return escape_pressed(ui).then_some(StatsOutcome::Done);
        }
        if self.tab.is_empty(data) {
            self.tab = StatsTab::General;
        }
        self.draw_tab(ui, atlas, lang, data, &gui);
        gui.title(ui, atlas, lang.tr("gui.stats"), gui.cx(), 20.0);
        let tab = self.buttons(ui, atlas, lang, data);
        if let Some(tab) = tab {
            self.tab = tab;
        }
        let done = self.done_button(ui, atlas, lang);
        (done || escape_pressed(ui)).then_some(StatsOutcome::Done)
    }

    fn draw_loading(&self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang, gui: &Gui) {
        let middle = (gui.height / 2.0).floor();
        let text = lang.tr("multiplayer.downloadingStats");
        gui.text(
            ui,
            atlas,
            text,
            TextPlacement::centered([gui.cx(), middle], WHITE),
        );
        let marker =
            LOADING_MARKERS[(self.ticks / TICKS_PER_MARKER) as usize % LOADING_MARKERS.len()];
        let at = [gui.cx(), middle + 18.0];
        gui.text(ui, atlas, marker, TextPlacement::centered(at, WHITE));
    }

    fn draw_tab(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        data: &StatsData,
        gui: &Gui,
    ) {
        let tab = self.tab;
        let rows = match tab {
            StatsTab::General => data.general.len(),
            StatsTab::Blocks => data.blocks.len(),
            StatsTab::Items => data.items.len(),
            StatsTab::Mobs => data.mobs.len(),
        };
        let (slot_height, header) = match tab {
            StatsTab::General => (GENERAL_HEIGHT, 0.0),
            StatsTab::Mobs => (MOB_HEIGHT, 0.0),
            StatsTab::Blocks | StatsTab::Items => (SLOT_HEIGHT, 20.0),
        };
        let frame = ListFrame {
            slot_height,
            header,
            rows,
        };
        let offset = frame.scrolled(ui, gui, self.offsets[tab.index()], &mut self.bar);
        self.offsets[tab.index()] = offset;
        match tab {
            StatsTab::General => draw_general(ui, atlas, lang, gui, &data.general, offset),
            StatsTab::Mobs => draw_mobs(ui, atlas, lang, gui, &data.mobs, offset),
            StatsTab::Blocks => {
                self.draw_table(ui, atlas, lang, gui, (Table::Blocks, &data.blocks), offset)
            }
            StatsTab::Items => {
                self.draw_table(ui, atlas, lang, gui, (Table::Items, &data.items), offset)
            }
        }
    }

    fn draw_table(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        gui: &Gui,
        (table, rows): (Table, &[ObjectRow]),
        offset: f32,
    ) {
        let slot = usize::from(table == Table::Items);
        if self.orders[slot].len() != rows.len() {
            self.orders[slot] = self.sorts[slot].order(rows);
        }
        let sheet = ui.menu_art().stats_icons;
        let mut view = TableView {
            table,
            rows,
            order: &self.orders[slot],
            sort: &mut self.sorts[slot],
            pressed: &mut self.pressed,
            sheet,
            offset,
        };
        let resorted = view.show(ui, atlas, gui);
        if let Some(pointer) = ui.pointer() {
            view.tooltip(ui, atlas, lang, gui, gui.to_gui(pointer));
        }
        if resorted {
            self.orders[slot] = self.sorts[slot].order(rows);
        }
    }

    /// Draws the tab buttons and returns the one that was clicked.
    fn buttons(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        data: &StatsData,
    ) -> Option<StatsTab> {
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let mut clicked = None;
        ui.push_id("stats-tabs");
        for tab in StatsTab::ALL {
            let id = ui.id(tab.key());
            let enabled = !tab.is_empty(data);
            let rect = stats_tab_rect(viewport, tab);
            if gui.button(ui, id, rect, atlas, lang.tr(tab.key()), enabled) {
                clicked = Some(tab);
            }
        }
        ui.pop_id();
        clicked
    }

    fn done_button(&self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> bool {
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        ui.push_id("stats");
        let id = ui.id("done");
        let done = gui.button(
            ui,
            id,
            stats_done_rect(viewport),
            atlas,
            lang.tr("gui.done"),
            true,
        );
        ui.pop_id();
        done
    }
}
