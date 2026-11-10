// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of one server list row (MCP `ServerListEntryNormal.drawEntry`).
//!
//! The name and the description are drawn without shadow (`drawString`), the
//! description wrapped at `listWidth - 32 - 2` into at most two lines with the
//! server's `§` colours. The icon is the server's favicon or
//! `unknown_server.png`; the latency cell comes from `icons.png`; the arrows
//! of a hovered row from `server_selection.png`. Sheets the atlas lacks fall
//! back to flat shapes and text glyphs.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::legacy_text::{parse_legacy, wrap_legacy};
use crate::styled_text::styled_width;
use crate::text::FontAtlas;
use crate::widgets::tooltip::{tooltip, TooltipRow};

use super::common::{Gui, DIM_GRAY, LINE_HEIGHT, WHITE};
use super::hover_arrows::{draw_hover_arrows, HoverArrows};
use super::server_row::{latency_level, RowStatus, ServerRow, ENTRY_HEIGHT, ENTRY_WIDTH};

const BAR_ON: Color = [0.33, 0.9, 0.33, 1.0];
const BAR_OFF: Color = [0.25, 0.25, 0.25, 1.0];
const RED: Color = [0.667, 0.0, 0.0, 1.0];
const ICON_FILL: Color = [0.13, 0.13, 0.17, 1.0];
/// The cell of `icons.png` that is the red X (`l = 5`).
const NO_CONNECTION_LEVEL: u8 = 5;

/// What a row draw needs besides the row itself.
pub(super) struct RowContext<'a> {
    pub gui: &'a Gui,
    pub atlas: &'a FontAtlas,
    pub lang: &'a Lang,
    pub ticks: u32,
}

/// The decorations of a row.
#[derive(Debug, Clone, Copy)]
pub(super) struct RowFlags {
    /// The row's position in the list, which offsets the pinging animation.
    pub index: usize,
    /// The pointer is over the row (`ServerListEntryNormal`'s `isSelected`).
    pub hovered: bool,
    /// The row is the selected one (outline box).
    pub selected: bool,
    pub can_up: bool,
    pub can_down: bool,
}

impl RowContext<'_> {
    /// Draws `row` with its top-left corner at GUI `at` and queues its
    /// tooltip in `tips` when the pointer is on the latency icon or the
    /// player count.
    pub(super) fn draw(
        &self,
        ui: &mut Ui,
        row: &ServerRow,
        at: [f32; 2],
        flags: RowFlags,
        tips: &mut Vec<TooltipRow>,
    ) {
        let [x, y] = at;
        if flags.selected {
            self.draw_selection_box(ui, at);
        }
        self.gui.legacy_text(
            ui,
            self.atlas,
            &row.name,
            ([x + 35.0, y + 1.0], WHITE, false),
        );
        self.draw_description(ui, &row.status, [x + 35.0, y + 12.0]);
        let info = self.draw_info(ui, &row.status, [x, y]);
        self.draw_latency(ui, &row.status, [x + ENTRY_WIDTH - 15.0, y], flags.index);
        self.draw_icon(ui, row, at);
        let pointer = ui.pointer().map(|p| self.gui.to_gui(p));
        let Some(pointer) = pointer.filter(|_| flags.hovered) else {
            return;
        };
        let rel = [pointer[0] - x, pointer[1] - y];
        self.draw_arrows(ui, at, flags, rel);
        if Rect::new(x + ENTRY_WIDTH - 15.0, y, 10.0, 8.0).contains(pointer) {
            tips.push(self.latency_tip(&row.status));
        } else if info.contains(pointer) {
            tips.extend(self.sample_tips(&row.status));
        }
    }

    /// The outline of `GuiSlot.drawSelectionBox`: a grey box with a black one
    /// inset by one unit.
    fn draw_selection_box(&self, ui: &mut Ui, at: [f32; 2]) {
        let [x, y] = at;
        let gui = self.gui;
        ui.draw().rect(
            gui.rect(x - 2.0, y - 2.0, ENTRY_WIDTH - 1.0, ENTRY_HEIGHT + 4.0),
            DIM_GRAY,
        );
        ui.draw().rect(
            gui.rect(x - 1.0, y - 1.0, ENTRY_WIDTH - 3.0, ENTRY_HEIGHT + 2.0),
            [0.0, 0.0, 0.0, 1.0],
        );
    }

    /// The favicon, else `unknown_server.png`, else a dark square.
    fn draw_icon(&self, ui: &mut Ui, row: &ServerRow, at: [f32; 2]) {
        let rect = self.gui.rect(at[0], at[1], 32.0, 32.0);
        match row.favicon.or(ui.menu_art().unknown_server) {
            Some(sheet) => {
                ui.draw()
                    .image(sheet.sub(0, 0, sheet.width, sheet.height), rect, [1.0; 4])
            }
            None => ui.draw().rect(rect, ICON_FILL),
        }
    }

    /// The description (or the failure text), at most two wrapped lines.
    fn draw_description(&self, ui: &mut Ui, status: &RowStatus, at: [f32; 2]) {
        let text = match status {
            RowStatus::Online { motd, .. } | RowStatus::VersionMismatch { motd, .. } => {
                motd.clone()
            }
            RowStatus::Unreachable => {
                format!("\u{a7}4{}", self.lang.tr("umicraft.server.unreachable"))
            }
            RowStatus::Pinging => return,
        };
        let width = ENTRY_WIDTH - 32.0 - 2.0;
        for (i, line) in wrap_legacy(self.atlas, &text, width)
            .iter()
            .take(2)
            .enumerate()
        {
            let line_at = [at[0], at[1] + i as f32 * LINE_HEIGHT];
            self.gui
                .legacy_text(ui, self.atlas, line, (line_at, DIM_GRAY, false));
        }
    }

    /// Draws the player count (or the server's version in dark red) and
    /// returns its box.
    fn draw_info(&self, ui: &mut Ui, status: &RowStatus, origin: [f32; 2]) -> Rect {
        let text = match status {
            RowStatus::Online { players, max, .. } => {
                format!("\u{a7}7{players}\u{a7}8/\u{a7}7{max}")
            }
            RowStatus::VersionMismatch { version, .. } => format!("\u{a7}4{version}"),
            RowStatus::Pinging | RowStatus::Unreachable => return Rect::ZERO,
        };
        let runs = parse_legacy(&text, DIM_GRAY);
        let width = styled_width(self.atlas, &runs, 1.0);
        let x = origin[0] + ENTRY_WIDTH - width - 15.0 - 2.0;
        self.gui.legacy_text(
            ui,
            self.atlas,
            &text,
            ([x, origin[1] + 1.0], DIM_GRAY, false),
        );
        Rect::new(x, origin[1], width, 8.0)
    }

    /// The latency icon of `icons.png` (cell `u = k * 10`, `v = 176 + l * 8`,
    /// 10x8) or five drawn bars without the sheet.
    fn draw_latency(&self, ui: &mut Ui, status: &RowStatus, at: [f32; 2], index: usize) {
        let (pinging, level) = match status {
            RowStatus::Pinging => (true, self.ping_phase(index)),
            RowStatus::Online { latency_ms, .. } => (false, latency_level(*latency_ms)),
            RowStatus::Unreachable | RowStatus::VersionMismatch { .. } => {
                (false, NO_CONNECTION_LEVEL)
            }
        };
        let rect = self.gui.rect(at[0], at[1], 10.0, 8.0);
        if let Some(sheet) = ui.menu_art().icons {
            let u = if pinging { 10 } else { 0 };
            let src = sheet.sub(u, 176 + u32::from(level) * 8, 10, 8);
            ui.draw().image(src, rect, [1.0; 4]);
            return;
        }
        let lit = usize::from(NO_CONNECTION_LEVEL.saturating_sub(level)) % 6;
        for bar in 0..5 {
            let color = if level >= NO_CONNECTION_LEVEL {
                RED
            } else if bar < lit {
                BAR_ON
            } else {
                BAR_OFF
            };
            let height = 2.0 + bar as f32 * 1.5;
            ui.draw().rect(
                self.gui
                    .rect(at[0] + bar as f32 * 2.0, at[1] + 8.0 - height, 1.0, height),
                color,
            );
        }
    }

    /// `(time / 100 + index * 2) & 7` folded to 0..=4; two ticks are 100 ms.
    fn ping_phase(&self, index: usize) -> u8 {
        let phase = ((self.ticks / 2 + index as u32 * 2) & 7) as u8;
        if phase > 4 {
            8 - phase
        } else {
            phase
        }
    }

    /// The hover shade and the arrows of `server_selection.png`; `rel` is
    /// the pointer relative to the row's corner.
    fn draw_arrows(&self, ui: &mut Ui, at: [f32; 2], flags: RowFlags, rel: [f32; 2]) {
        let arrows = HoverArrows {
            can_up: flags.can_up,
            can_down: flags.can_down,
            rel,
        };
        draw_hover_arrows(ui, (self.gui, self.atlas), at, arrows);
    }

    /// The text of the hover box over the latency icon (`s1`).
    fn latency_tip(&self, status: &RowStatus) -> TooltipRow {
        let text = match status {
            RowStatus::Pinging => self.lang.tr("umicraft.server.pinging").to_string(),
            RowStatus::Online { latency_ms, .. } => format!("{latency_ms}ms"),
            RowStatus::Unreachable => self.lang.tr("umicraft.server.noConnection").to_string(),
            RowStatus::VersionMismatch {
                client_outdated, ..
            } => self
                .lang
                .tr(if *client_outdated {
                    "umicraft.server.clientOutdated"
                } else {
                    "umicraft.server.serverOutdated"
                })
                .to_string(),
        };
        TooltipRow {
            text,
            rgb: 0xFF_FF_FF,
        }
    }

    /// The player list of the hover box over the player count: the sample
    /// names, then "... and N more ..." when the server hides some.
    fn sample_tips(&self, status: &RowStatus) -> Vec<TooltipRow> {
        let (players, sample) = match status {
            RowStatus::Online {
                players, sample, ..
            }
            | RowStatus::VersionMismatch {
                players, sample, ..
            } => (*players as usize, sample),
            RowStatus::Pinging | RowStatus::Unreachable => return Vec::new(),
        };
        let mut lines: Vec<String> = sample
            .iter()
            .filter(|line| !line.is_empty())
            .cloned()
            .collect();
        if !lines.is_empty() && sample.len() < players {
            let hidden = (players - sample.len()).to_string();
            lines.push(self.lang.tr_args("umicraft.server.andMore", &[&hidden]));
        }
        lines
            .into_iter()
            .map(|text| TooltipRow {
                text,
                rgb: 0xFF_FF_FF,
            })
            .collect()
    }
}

/// Draws the queued tooltip at the pointer.
pub(super) fn draw_tips(ui: &mut Ui, atlas: &FontAtlas, gui: &Gui, tips: &[TooltipRow]) {
    if let Some(pointer) = ui.pointer() {
        let viewport = ui.viewport();
        tooltip(ui, atlas, viewport, gui.scale, pointer, tips);
    }
}
