// SPDX-License-Identifier: MIT OR Apache-2.0

//! The multiplayer screen (MCP `GuiMultiplayer` with `ServerSelectionList`).
//!
//! GUI units, `cx = width / 2`: the title `multiplayer.title` at `(cx, 20)`;
//! the list from `y = 32` to `height - 64`, rows 36 units apart starting 4
//! below the top (see [`super::server_row`]); two button rows at
//! `height - 52` (Join 100, Direct Connect 100, Add 100) and `height - 28`
//! (Edit 70, Delete 70, Refresh 70, Cancel 75). The scrollbar sits at
//! `cx + 154`. Difference: the LAN scan entries are not shown (vanilla's "Scanning for games on your local network" row).
//!
//! Double click: vanilla compares wall-clock times (250 ms); the screen
//! counts [`ServerListScreen::tick`] calls, so two clicks on one row within
//! five ticks join.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::input::Key;
use crate::text::FontAtlas;
use crate::widgets::tooltip::TooltipRow;
use crate::widgets::util::last_primary_press;

use super::background::{draw_list_background, draw_list_overlay, draw_menu_background};
use super::common::{
    draw_scrollbar, escape_pressed, key_pressed, wheel_scrolled, Gui, ScrollbarDrag,
};
use super::row_draw::{draw_tips, RowContext, RowFlags};
pub use super::server_row::{latency_level, RowStatus, ServerRow};
use super::server_row::{zone_at, RowZone, ENTRY_HEIGHT, ENTRY_LEFT, ENTRY_WIDTH, SLOT_HEIGHT};

/// The ticks within which a second click on a row joins (250 ms).
const DOUBLE_CLICK_TICKS: u32 = 5;
/// Space above the first row inside the list.
const LIST_PADDING: f32 = 4.0;
/// The top of the list in GUI units.
const LIST_TOP: f32 = 32.0;

/// What the player chose on the server list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerListOutcome {
    /// Connect to row `i` (Join button, double click, play arrow or Enter).
    Join(usize),
    /// Open the Direct Connect screen.
    DirectConnect,
    /// Open the Add Server screen.
    Add,
    /// Edit row `i`.
    Edit(usize),
    /// Ask to delete row `i` (the caller shows the confirmation).
    Delete(usize),
    /// Ping every server again.
    Refresh,
    /// Leave the screen (Cancel or Escape).
    Cancel,
    /// Swap row `i` with the one above; the selection follows the entry.
    MoveUp(usize),
    /// Swap row `i` with the one below; the selection follows the entry.
    MoveDown(usize),
}

/// The rectangles of the server list screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerListLayout {
    /// The scrolling list area.
    pub list: Rect,
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

/// Computes the vanilla layout for `viewport`.
#[must_use]
pub fn server_list_layout(viewport: Rect) -> ServerListLayout {
    let gui = Gui::new(viewport);
    let (cx, h) = (gui.cx(), gui.height);
    ServerListLayout {
        list: gui.rect(0.0, LIST_TOP, gui.width, h - 64.0 - LIST_TOP),
        join: gui.rect(cx - 154.0, h - 52.0, 100.0, 20.0),
        direct: gui.rect(cx - 50.0, h - 52.0, 100.0, 20.0),
        add: gui.rect(cx + 54.0, h - 52.0, 100.0, 20.0),
        edit: gui.rect(cx - 154.0, h - 28.0, 70.0, 20.0),
        delete: gui.rect(cx - 74.0, h - 28.0, 70.0, 20.0),
        refresh: gui.rect(cx + 4.0, h - 28.0, 70.0, 20.0),
        cancel: gui.rect(cx + 80.0, h - 28.0, 75.0, 20.0),
    }
}

/// The selection, scroll and click history of the list.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ServerListScreen {
    offset: f32,
    selected: Option<usize>,
    ticks: u32,
    last_click: Option<(usize, u32)>,
    bar: ScrollbarDrag,
}

impl ServerListScreen {
    /// Creates the screen with nothing selected.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the click clock and the ping animation by one game tick.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    /// The selected row.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Selects a row (for example the one just added).
    pub fn set_selected(&mut self, selected: Option<usize>) {
        self.selected = selected;
    }

    /// The scroll offset in GUI units.
    #[must_use]
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Draws one frame of `rows` and returns the player's choice. The sheets
    /// of the rows come from [`Ui::menu_art`]; without them the icons and
    /// arrows are drawn as flat shapes.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        rows: &[ServerRow],
    ) -> Option<ServerListOutcome> {
        draw_menu_background(ui);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = server_list_layout(viewport);
        self.selected = self.selected.filter(|&index| index < rows.len());

        let content = rows.len() as f32 * SLOT_HEIGHT + LIST_PADDING;
        let key_outcome = self.keyboard(ui, rows.len());
        self.offset = wheel_scrolled(ui, &gui, layout.list, content, self.offset);
        let bar = [gui.cx() + 154.0, LIST_TOP, gui.height - 96.0];
        let max = (content - bar[2]).max(0.0);
        self.offset = self
            .bar
            .apply(ui, gui.scale, bar, (content, max), self.offset);
        ui.push_id("server-list");
        let list_id = ui.id("rows");
        let clicked = ui.response(list_id, layout.list).clicked && !self.bar.is_active();
        let click = clicked.then(|| last_primary_press(ui)).flatten();
        let outcome = click
            .and_then(|position| self.click_row(&gui, position, rows.len()))
            .or(key_outcome);
        draw_list_background(ui, layout.list, self.offset * gui.scale);
        self.draw_rows(ui, atlas, lang, rows, &gui);
        draw_list_overlay(ui, layout.list);
        draw_scrollbar(ui, &gui, bar, content, self.offset);
        gui.title(ui, atlas, lang.tr("multiplayer.title"), gui.cx(), 20.0);
        let button = self.buttons(ui, atlas, lang, &gui, &layout, !rows.is_empty());
        ui.pop_id();

        outcome
            .or(button)
            .or_else(|| escape_pressed(ui).then_some(ServerListOutcome::Cancel))
    }

    /// Maps a press at `position` (pixels) to a selection and an outcome.
    fn click_row(
        &mut self,
        gui: &Gui,
        position: [f32; 2],
        count: usize,
    ) -> Option<ServerListOutcome> {
        let pointer = gui.to_gui(position);
        let index = row_at(gui, pointer, self.offset).filter(|&index| index < count)?;
        let corner = row_corner(gui, index, self.offset);
        let zone = zone_at(
            [pointer[0] - corner[0], pointer[1] - corner[1]],
            index > 0,
            index + 1 < count,
        );
        self.selected = Some(index);
        match zone {
            RowZone::Play => Some(ServerListOutcome::Join(index)),
            RowZone::Up => {
                self.selected = Some(index - 1);
                Some(ServerListOutcome::MoveUp(index))
            }
            RowZone::Down => {
                self.selected = Some(index + 1);
                Some(ServerListOutcome::MoveDown(index))
            }
            RowZone::Body => self.double_click(index),
        }
    }

    /// Records a click on a row body; the second one in time joins.
    fn double_click(&mut self, index: usize) -> Option<ServerListOutcome> {
        let double = self.last_click.is_some_and(|(row, tick)| {
            row == index && self.ticks.wrapping_sub(tick) <= DOUBLE_CLICK_TICKS
        });
        self.last_click = if double {
            None
        } else {
            Some((index, self.ticks))
        };
        double.then_some(ServerListOutcome::Join(index))
    }

    /// The keys of `GuiMultiplayer.keyTyped`: F5 refreshes; with a row
    /// selected, Up and Down move the selection (Up on the first row and
    /// Down on the last clear it), Shift moves the entry itself, and Enter
    /// joins. With no selection the arrows do nothing. Each step also scrolls
    /// the list by one slot, as vanilla's `scrollBy` does.
    fn keyboard(&mut self, ui: &Ui, count: usize) -> Option<ServerListOutcome> {
        if key_pressed(ui, Key::F5) {
            return Some(ServerListOutcome::Refresh);
        }
        let index = self.selected?;
        let shift = ui.input().modifiers.shift;
        if key_pressed(ui, Key::ArrowUp) {
            return self.step(index, false, shift, count);
        }
        if key_pressed(ui, Key::ArrowDown) {
            return self.step(index, true, shift, count);
        }
        key_pressed(ui, Key::Enter).then_some(ServerListOutcome::Join(index))
    }

    /// One Up (`down == false`) or Down step from `index`.
    fn step(
        &mut self,
        index: usize,
        down: bool,
        shift: bool,
        count: usize,
    ) -> Option<ServerListOutcome> {
        let target = if down {
            (index + 1 < count).then_some(index + 1)
        } else {
            index.checked_sub(1)
        };
        let Some(target) = target else {
            // Past either end: a plain step deselects, a shifted one stays.
            if !shift {
                self.selected = None;
            }
            return None;
        };
        self.selected = Some(target);
        self.offset += if down { SLOT_HEIGHT } else { -SLOT_HEIGHT };
        match (shift, down) {
            (false, _) => None,
            (true, true) => Some(ServerListOutcome::MoveDown(index)),
            (true, false) => Some(ServerListOutcome::MoveUp(index)),
        }
    }

    fn draw_rows(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        rows: &[ServerRow],
        gui: &Gui,
    ) {
        let layout = server_list_layout(ui.viewport());
        let context = RowContext {
            gui,
            atlas,
            lang,
            ticks: self.ticks,
        };
        let pointer = ui
            .pointer()
            .filter(|&p| layout.list.contains(p))
            .map(|p| gui.to_gui(p));
        let hovered = pointer.and_then(|p| row_at(gui, p, self.offset));
        let mut tips: Vec<TooltipRow> = Vec::new();
        ui.push_clip(layout.list);
        for (index, row) in rows.iter().enumerate() {
            let at = row_corner(gui, index, self.offset);
            if at[1] + ENTRY_HEIGHT < LIST_TOP || at[1] > gui.height - 64.0 {
                continue;
            }
            let flags = RowFlags {
                index,
                hovered: hovered == Some(index),
                selected: self.selected == Some(index),
                can_up: index > 0,
                can_down: index + 1 < rows.len(),
            };
            context.draw(ui, row, at, flags, &mut tips);
        }
        ui.pop_clip();
        draw_tips(ui, atlas, gui, &tips);
    }

    fn buttons(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        gui: &Gui,
        layout: &ServerListLayout,
        any: bool,
    ) -> Option<ServerListOutcome> {
        let chosen = self.selected;
        let has = chosen.is_some() && any;
        let press = |ui: &mut Ui, id: &str, rect: Rect, key: &str, enabled: bool| {
            let id = ui.id(id);
            gui.button(ui, id, rect, atlas, lang.tr(key), enabled)
        };
        let join = press(ui, "join", layout.join, "selectServer.select", has);
        let direct = press(ui, "direct", layout.direct, "selectServer.direct", true);
        let add = press(ui, "add", layout.add, "selectServer.add", true);
        let edit = press(ui, "edit", layout.edit, "selectServer.edit", has);
        let delete = press(ui, "delete", layout.delete, "selectServer.delete", has);
        let refresh = press(ui, "refresh", layout.refresh, "selectServer.refresh", true);
        let cancel = press(ui, "cancel", layout.cancel, "gui.cancel", true);
        let index = chosen.unwrap_or(0);
        [
            (join, ServerListOutcome::Join(index)),
            (direct, ServerListOutcome::DirectConnect),
            (add, ServerListOutcome::Add),
            (edit, ServerListOutcome::Edit(index)),
            (delete, ServerListOutcome::Delete(index)),
            (refresh, ServerListOutcome::Refresh),
            (cancel, ServerListOutcome::Cancel),
        ]
        .into_iter()
        .find_map(|(clicked, outcome)| clicked.then_some(outcome))
    }
}

/// The top-left corner of row `index` in GUI units.
fn row_corner(gui: &Gui, index: usize, offset: f32) -> [f32; 2] {
    [
        gui.cx() + ENTRY_LEFT,
        LIST_TOP + LIST_PADDING + index as f32 * SLOT_HEIGHT - offset,
    ]
}

/// The row under a GUI position (`GuiSlot.getSlotIndexFromScreenCoords`).
fn row_at(gui: &Gui, pointer: [f32; 2], offset: f32) -> Option<usize> {
    let inside_x = (pointer[0] - gui.cx()).abs() <= ENTRY_WIDTH / 2.0;
    let relative = pointer[1] - LIST_TOP - LIST_PADDING + offset;
    (inside_x && relative >= 0.0).then(|| (relative / SLOT_HEIGHT) as usize)
}

#[cfg(test)]
mod row_tests;
#[cfg(test)]
mod tests;
