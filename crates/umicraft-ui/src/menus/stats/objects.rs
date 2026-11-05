// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Blocks and Items tables of `GuiStats` (`StatsBlock`, `StatsItem`):
//! three counters per item, sortable by clicking a column header.
//!
//! Vanilla draws the headers with the `stats_icons.png` sprites: a grey
//! 18 unit button per column carrying the glyph of its counter, and an arrow
//! left of it on the sorted column. Clicking a header sorts that column
//! descending, again ascending, a third time not at all; rows without the
//! statistic come last and ties fall back to the item id.

use std::cmp::Ordering;

use umicraft_core::lang::Lang;

use super::list::{hover_label, ListFrame};
use super::model::ObjectRow;
use crate::draw::SpriteSheet;
use crate::input::{InputEvent, PointerButton};
use crate::menus::common::{Gui, TextPlacement, WHITE};
use crate::text::FontAtlas;
use crate::widgets::draw_item;
use crate::widgets::tooltip::text_color;
use crate::Ui;

/// Row height and header padding of the tables.
pub(super) const SLOT_HEIGHT: f32 = 20.0;
const HEADER: f32 = 20.0;
/// Right edges of the three value columns, from the row column.
const COLUMN_RIGHT: [f32; 3] = [115.0, 165.0, 215.0];
/// The size of a sprite of `stats_icons.png`.
const SPRITE: f32 = 18.0;
/// The click zones of the headers, from the left edge of the 220 unit list.
const CLICK_ZONES: [(f32, f32); 3] = [(79.0, 115.0), (129.0, 165.0), (179.0, 215.0)];
/// Distance of the sort arrow from the row column.
const ARROW_X: [f32; 3] = [79.0, 129.0, 179.0];
/// Odd rows are dimmer (`0x909090`); even rows are white.
const DIM_ROW: u32 = 0x90_90_90;
/// Item cell: the icon frame and the item inside it.
const ICON_X: f32 = 40.0;

/// Which table is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Table {
    /// Crafted, used, mined.
    Blocks,
    /// Depleted, crafted, used.
    Items,
}

impl Table {
    /// The `stats_icons.png` cell `(u, v)` of each column glyph.
    fn glyphs(self) -> [(u32, u32); 3] {
        match self {
            Self::Blocks => [(18, 18), (36, 18), (54, 18)],
            Self::Items => [(72, 18), (18, 18), (36, 18)],
        }
    }

    /// The language key of each column (the header tooltip).
    fn keys(self) -> [&'static str; 3] {
        match self {
            Self::Blocks => ["stat.crafted", "stat.used", "stat.mined"],
            Self::Items => ["stat.depleted", "stat.crafted", "stat.used"],
        }
    }
}

/// The sort of one table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Sort {
    column: Option<usize>,
    /// `-1` descending, `1` ascending.
    direction: i32,
}

impl Sort {
    /// What a click on `column` does (`GuiStats.Stats.func_148212_h`).
    fn clicked(&mut self, column: usize) {
        if self.column != Some(column) {
            *self = Self {
                column: Some(column),
                direction: -1,
            };
        } else if self.direction == -1 {
            self.direction = 1;
        } else {
            *self = Self::default();
        }
    }

    /// The order of `rows` under this sort, as indices.
    pub(super) fn order(&self, rows: &[ObjectRow]) -> Vec<usize> {
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&a, &b| self.compare(&rows[a], &rows[b]));
        order
    }

    fn compare(&self, a: &ObjectRow, b: &ObjectRow) -> Ordering {
        let cells = self.column.map(|c| (&a.cells[c], &b.cells[c]));
        let by_value = match cells {
            Some((Some(x), Some(y))) if x.raw != y.raw => {
                let ordering = x.raw.cmp(&y.raw);
                if self.direction < 0 {
                    ordering.reverse()
                } else {
                    ordering
                }
            }
            Some((None, Some(_))) => Ordering::Greater,
            Some((Some(_), None)) => Ordering::Less,
            _ => Ordering::Equal,
        };
        by_value.then(a.id.cmp(&b.id))
    }
}

/// What a frame of a table needs besides the rows.
pub(super) struct TableView<'a> {
    pub table: Table,
    pub rows: &'a [ObjectRow],
    pub order: &'a [usize],
    pub sort: &'a mut Sort,
    /// The header column the pointer is holding down.
    pub pressed: &'a mut Option<usize>,
    pub sheet: Option<SpriteSheet>,
    pub offset: f32,
}

impl TableView<'_> {
    const fn frame(&self) -> ListFrame {
        ListFrame {
            slot_height: SLOT_HEIGHT,
            header: HEADER,
            rows: self.rows.len(),
        }
    }

    /// Handles header presses, then draws the table; returns whether the sort
    /// changed so the caller recomputes the order.
    pub(super) fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas, gui: &Gui) -> bool {
        let resorted = self.press_headers(ui, gui);
        let frame = self.frame();
        frame.draw(ui, gui, self.offset, |ui| {
            self.draw_header(ui, gui);
            self.draw_rows(ui, atlas, gui, frame);
        });
        resorted
    }

    fn press_headers(&mut self, ui: &Ui, gui: &Gui) -> bool {
        let events = &ui.input().events;
        if events
            .iter()
            .any(|e| matches!(e, InputEvent::PointerReleased { .. }))
        {
            *self.pressed = None;
        }
        let mut resorted = false;
        for event in events {
            let InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            } = event
            else {
                continue;
            };
            let point = gui.to_gui(*position);
            if let Some(column) = self.header_column(gui, point) {
                self.sort.clicked(column);
                *self.pressed = Some(column);
                resorted = true;
            }
        }
        resorted
    }

    /// The header column at GUI point `point` (`func_148132_a`).
    fn header_column(&self, gui: &Gui, point: [f32; 2]) -> Option<usize> {
        let list_left = (gui.width - 220.0).div_euclid(2.0);
        let in_list = point[1] >= super::list::LIST_TOP
            && point[1] <= ListFrame::bottom(gui)
            && self.frame().row_at(point, self.offset).is_none()
            && point[1] < ListFrame::header_top(self.offset) + HEADER;
        let x = point[0] - list_left;
        in_list
            .then(|| CLICK_ZONES.iter().position(|&(lo, hi)| x >= lo && x < hi))
            .flatten()
    }

    fn draw_header(&self, ui: &mut Ui, gui: &Gui) {
        let left = ListFrame::left(gui);
        let top = ListFrame::header_top(self.offset);
        for (column, (glyph, right)) in self.table.glyphs().iter().zip(COLUMN_RIGHT).enumerate() {
            let held = *self.pressed == Some(column);
            let nudge = if held { 1.0 } else { 0.0 };
            let x = left + right - SPRITE;
            self.sprite(ui, gui, [x, top + 1.0], (0, if held { 0 } else { 18 }));
            self.sprite(ui, gui, [x + nudge, top + 1.0 + nudge], *glyph);
        }
        if let Some(column) = self.sort.column {
            let u = if self.sort.direction == 1 { 36 } else { 18 };
            self.sprite(ui, gui, [left + ARROW_X[column], top + 1.0], (u, 0));
        }
    }

    fn draw_rows(&self, ui: &mut Ui, atlas: &FontAtlas, gui: &Gui, frame: ListFrame) {
        let left = ListFrame::left(gui);
        for (shown, &index) in self.order.iter().enumerate() {
            let y = frame.row_top(shown, self.offset);
            if y + SLOT_HEIGHT < super::list::LIST_TOP || y > ListFrame::bottom(gui) {
                continue;
            }
            let row = &self.rows[index];
            self.sprite(ui, gui, [left + ICON_X + 1.0, y + 1.0], (0, 0));
            let cell = gui.rect(left + ICON_X + 2.0, y + 2.0, 16.0, 16.0);
            draw_item(ui.draw(), atlas, cell, &row.icon);
            let color = if shown % 2 == 0 {
                WHITE
            } else {
                text_color(DIM_ROW)
            };
            for (cell, right) in row.cells.iter().zip(COLUMN_RIGHT) {
                let text = cell.as_ref().map_or("-", |c| c.text.as_str());
                let width = atlas.measure(text, gui.scale) / gui.scale;
                let at = [left + right - width, y + 5.0];
                gui.text(ui, atlas, text, TextPlacement::left(at, color));
            }
        }
    }

    /// Draws the 18 unit sprite at cell `(u, v)` of `stats_icons.png`, or a
    /// grey square for the button background without the texture.
    fn sprite(&self, ui: &mut Ui, gui: &Gui, at: [f32; 2], (u, v): (u32, u32)) {
        let dst = gui.rect(at[0], at[1], SPRITE, SPRITE);
        match self.sheet {
            Some(sheet) => ui.draw().image(sheet.sub(u, v, 18, 18), dst, [1.0; 4]),
            None if (u, v) == (0, 0) || (u, v) == (0, 18) => {
                ui.draw().rect(dst, [0.55, 0.55, 0.55, 1.0]);
            }
            None => {}
        }
    }

    /// The hover label under `point` (GUI units): the item name over an icon,
    /// the counter name over a header.
    pub(super) fn tooltip(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        gui: &Gui,
        point: [f32; 2],
    ) {
        if point[1] < super::list::LIST_TOP || point[1] > ListFrame::bottom(gui) {
            return;
        }
        let left = ListFrame::left(gui);
        let text = match self.frame().row_at(point, self.offset) {
            Some(shown) => {
                let over_icon = (left + ICON_X..=left + ICON_X + 20.0).contains(&point[0]);
                over_icon
                    .then(|| self.rows[self.order[shown]].name.clone())
                    .filter(|name| !name.is_empty())
            }
            None => COLUMN_RIGHT
                .iter()
                .position(|&right| (left + right - SPRITE..=left + right).contains(&point[0]))
                .map(|column| lang.tr(self.table.keys()[column]).trim().to_string())
                .filter(|label| !label.is_empty()),
        };
        if let Some(text) = text {
            hover_label(ui, atlas, gui, point, &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::stats::model::StatCell;
    use crate::widgets::SlotItem;

    fn cell(raw: i32) -> Option<StatCell> {
        Some(StatCell {
            raw,
            text: raw.to_string(),
        })
    }

    fn row(id: i32, cells: [Option<StatCell>; 3]) -> ObjectRow {
        ObjectRow {
            id,
            icon: SlotItem {
                key: 0,
                count: 1,
                damage: 0,
                max_durability: 0,
                glint: false,
            },
            name: String::new(),
            cells,
        }
    }

    fn rows() -> Vec<ObjectRow> {
        vec![
            row(1, [cell(5), cell(1), None]),
            row(2, [None, cell(9), cell(2)]),
            row(3, [cell(7), cell(9), cell(0)]),
        ]
    }

    #[test]
    fn no_sort_keeps_the_item_id_order() {
        assert_eq!(Sort::default().order(&rows()), [0, 1, 2]);
    }

    #[test]
    fn a_click_sorts_descending_then_ascending_then_clears() {
        let mut sort = Sort::default();
        sort.clicked(0);
        // 7, 5, then the row without the statistic
        assert_eq!(sort.order(&rows()), [2, 0, 1]);
        sort.clicked(0);
        assert_eq!(sort.order(&rows()), [0, 2, 1]);
        sort.clicked(0);
        assert_eq!(sort, Sort::default());
    }

    #[test]
    fn rows_without_the_statistic_come_last_in_both_directions() {
        let mut sort = Sort::default();
        sort.clicked(2);
        assert_eq!(sort.order(&rows())[2], 0);
        sort.clicked(2);
        assert_eq!(sort.order(&rows())[2], 0);
    }

    #[test]
    fn ties_fall_back_to_the_item_id() {
        let mut sort = Sort::default();
        sort.clicked(1);
        // 9 and 9 tie: id 2 before id 3, then 1
        assert_eq!(sort.order(&rows()), [1, 2, 0]);
    }

    #[test]
    fn another_column_starts_descending() {
        let mut sort = Sort::default();
        sort.clicked(0);
        sort.clicked(0);
        sort.clicked(1);
        assert_eq!(sort.order(&rows()), [1, 2, 0]);
    }
}
