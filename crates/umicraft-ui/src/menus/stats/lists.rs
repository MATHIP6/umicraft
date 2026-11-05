// SPDX-License-Identifier: MIT OR Apache-2.0

//! The General and Mobs lists of `GuiStats` (`StatsGeneral`,
//! `StatsMobsList`): plain rows without selection.

use umicraft_core::lang::Lang;

use super::list::ListFrame;
use super::model::{MobRow, StatRow};
use crate::menus::common::{Gui, TextPlacement, WHITE};
use crate::text::FontAtlas;
use crate::widgets::tooltip::text_color;
use crate::Ui;

/// Row height of the general list.
pub(super) const GENERAL_HEIGHT: f32 = 10.0;
/// Row height of the mobs list (`FONT_HEIGHT * 4`).
pub(super) const MOB_HEIGHT: f32 = 36.0;
/// Right edge of the general values, from the row column.
const VALUE_RIGHT: f32 = 215.0;
const DIM_ROW: u32 = 0x90_90_90;
const NO_KILLS: u32 = 0x60_60_60;
/// Height of a text line.
const LINE: f32 = 9.0;

fn visible(y: f32, height: f32, gui: &Gui) -> bool {
    y + height >= super::list::LIST_TOP && y <= ListFrame::bottom(gui)
}

/// Draws the general counters: the label left, the value right.
pub(super) fn draw_general(
    ui: &mut Ui,
    atlas: &FontAtlas,
    lang: &Lang,
    gui: &Gui,
    rows: &[StatRow],
    offset: f32,
) {
    let frame = ListFrame {
        slot_height: GENERAL_HEIGHT,
        header: 0.0,
        rows: rows.len(),
    };
    frame.draw(ui, gui, offset, |ui| {
        let left = ListFrame::left(gui);
        for (index, row) in rows.iter().enumerate() {
            let y = frame.row_top(index, offset);
            if !visible(y, GENERAL_HEIGHT, gui) {
                continue;
            }
            let color = if index % 2 == 0 {
                WHITE
            } else {
                text_color(DIM_ROW)
            };
            let label = TextPlacement::left([left + 2.0, y + 1.0], color);
            gui.text(ui, atlas, lang.tr(&row.key), label);
            let width = atlas.measure(&row.value, gui.scale) / gui.scale;
            let at = [left + VALUE_RIGHT - width, y + 1.0];
            gui.text(ui, atlas, &row.value, TextPlacement::left(at, color));
        }
    });
}

/// Draws the mobs: the name and the two kill lines, dim when zero.
pub(super) fn draw_mobs(
    ui: &mut Ui,
    atlas: &FontAtlas,
    lang: &Lang,
    gui: &Gui,
    rows: &[MobRow],
    offset: f32,
) {
    let frame = ListFrame {
        slot_height: MOB_HEIGHT,
        header: 0.0,
        rows: rows.len(),
    };
    frame.draw(ui, gui, offset, |ui| {
        let left = ListFrame::left(gui);
        for (index, row) in rows.iter().enumerate() {
            let y = frame.row_top(index, offset);
            if !visible(y, MOB_HEIGHT, gui) {
                continue;
            }
            draw_mob(ui, atlas, lang, gui, row, [left, y]);
        }
    });
}

fn draw_mob(ui: &mut Ui, atlas: &FontAtlas, lang: &Lang, gui: &Gui, row: &MobRow, at: [f32; 2]) {
    let name = lang.tr(&format!("entity.{}.name", row.entity)).to_string();
    let kills = if row.kills == 0 {
        lang.tr_args("stat.entityKills.none", &[&name])
    } else {
        lang.tr_args("stat.entityKills", &[&row.kills.to_string(), &name])
    };
    let killed_by = if row.killed_by == 0 {
        lang.tr_args("stat.entityKilledBy.none", &[&name])
    } else {
        lang.tr_args("stat.entityKilledBy", &[&name, &row.killed_by.to_string()])
    };
    let shade = |count: i32| text_color(if count == 0 { NO_KILLS } else { DIM_ROW });
    let [x, y] = at;
    gui.text(
        ui,
        atlas,
        &name,
        TextPlacement::left([x + 2.0 - 10.0, y + 1.0], WHITE),
    );
    let kills_at = [x + 2.0, y + 1.0 + LINE];
    gui.text(
        ui,
        atlas,
        &kills,
        TextPlacement::left(kills_at, shade(row.kills)),
    );
    let by_at = [x + 2.0, y + 1.0 + LINE * 2.0];
    gui.text(
        ui,
        atlas,
        &killed_by,
        TextPlacement::left(by_at, shade(row.killed_by)),
    );
}
