// SPDX-License-Identifier: MIT OR Apache-2.0

//! One row of the server list (MCP `ServerListEntryNormal`): the view model
//! the client fills from its ping results, and how a row is drawn.
//!
//! A row is 32 GUI units high inside a 36-unit slot and 305 wide. Relative to
//! the row: the 32x32 icon at `(0, 0)`, the name at `(35, 1)`, up to two
//! description lines at `(35, 12)` and `(35, 21)` in `0x808080`, the
//! player count right-aligned ending 17 units from the right edge, and the
//! 10x8 latency icon at `(width - 15, 0)`.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::{Color, SpriteSheet};
use crate::geom::Rect;
use crate::text::FontAtlas;
use crate::widgets::tooltip::{tooltip, TooltipRow};

use super::common::{
    strip_formatting, wrap_lines, Gui, TextPlacement, DIM_GRAY, LINE_HEIGHT, WHITE,
};

/// Height of a row slot in GUI units (`ServerSelectionList` slot height).
pub(super) const SLOT_HEIGHT: f32 = 36.0;
/// Height of the drawn row, the slot minus its 4-unit gap.
pub(super) const ENTRY_HEIGHT: f32 = 32.0;
/// Width of a row (`ServerSelectionList.getListWidth`: 220 + 85).
pub(super) const ENTRY_WIDTH: f32 = 305.0;
/// The row's left edge relative to the screen centre (`-152 + 2`).
pub(super) const ENTRY_LEFT: f32 = -150.0;

const RED: Color = [0.667, 0.0, 0.0, 1.0];
const BAR_ON: Color = [0.33, 0.9, 0.33, 1.0];
const BAR_OFF: Color = [0.25, 0.25, 0.25, 1.0];
const ICON_FILL: Color = [0.13, 0.13, 0.17, 1.0];
const HOVER_SHADE: Color = [0.565, 0.565, 0.565, 0.627];

/// The state of one server's ping, as the client maps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowStatus {
    /// The ping is under way.
    Pinging,
    /// The server answered and speaks our protocol.
    Online {
        /// The server description (legacy `§` codes are removed).
        motd: String,
        /// Players online.
        players: u32,
        /// Player slots.
        max: u32,
        /// The server's version name, shown in the player tooltip.
        version: String,
        /// The round trip in milliseconds.
        latency_ms: u32,
        /// A sample of player names for the tooltip.
        sample: Vec<String>,
    },
    /// The ping failed (`Can't connect to server.`).
    Unreachable,
    /// The server answered with another protocol version.
    VersionMismatch,
}

/// A row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRow {
    /// The name the player gave the entry.
    pub name: String,
    /// The address (`host[:port]`).
    pub address: String,
    /// The ping state.
    pub status: RowStatus,
}

/// The latency bars level for `ms` (`ServerListEntryNormal.drawEntry`):
/// 0 is the best (under 150 ms) and 4 the worst (1000 ms and more).
#[must_use]
pub fn latency_level(ms: u32) -> u8 {
    match ms {
        0..=149 => 0,
        150..=299 => 1,
        300..=599 => 2,
        600..=999 => 3,
        _ => 4,
    }
}

/// The part of a row a pointer is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowZone {
    /// The play arrow of the icon: joins.
    Play,
    /// The up arrow: moves the entry up.
    Up,
    /// The down arrow: moves the entry down.
    Down,
    /// Anything else.
    Body,
}

/// The zone at `rel`, a GUI position relative to the row's corner
/// (`ServerListEntryNormal.mousePressed`).
pub(super) fn zone_at(rel: [f32; 2], can_up: bool, can_down: bool) -> RowZone {
    if rel[1] > ENTRY_HEIGHT || rel[0] >= ENTRY_HEIGHT {
        RowZone::Body
    } else if rel[0] > 16.0 {
        RowZone::Play
    } else if rel[1] < 16.0 && can_up {
        RowZone::Up
    } else if rel[1] > 16.0 && can_down {
        RowZone::Down
    } else {
        RowZone::Body
    }
}

/// What a row draw needs besides the row itself.
pub(super) struct RowContext<'a> {
    pub gui: &'a Gui,
    pub atlas: &'a FontAtlas,
    pub lang: &'a Lang,
    pub icons: Option<SpriteSheet>,
    pub ticks: u32,
}

/// The decorations of a row.
#[derive(Debug, Clone, Copy)]
pub(super) struct RowFlags {
    pub hovered: bool,
    pub selected: bool,
    pub can_up: bool,
    pub can_down: bool,
}

impl RowContext<'_> {
    /// Draws `row` with its top-left corner at GUI `(x, y)` and queues its
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
        let gui = self.gui;
        if flags.selected {
            ui.draw().rect(
                gui.rect(x - 2.0, y - 2.0, ENTRY_WIDTH - 1.0, ENTRY_HEIGHT + 4.0),
                DIM_GRAY,
            );
            ui.draw().rect(
                gui.rect(x - 1.0, y - 1.0, ENTRY_WIDTH - 3.0, ENTRY_HEIGHT + 2.0),
                [0.0, 0.0, 0.0, 1.0],
            );
        }
        ui.draw().rect(gui.rect(x, y, 32.0, 32.0), ICON_FILL);
        gui.text(
            ui,
            self.atlas,
            &strip_formatting(&row.name),
            TextPlacement::left([x + 35.0, y + 1.0], WHITE),
        );
        self.draw_description(ui, row, [x + 35.0, y + 12.0]);
        let info = self.draw_info(ui, row, [x, y]);
        self.draw_latency(ui, row, [x + ENTRY_WIDTH - 15.0, y]);
        if flags.hovered || flags.selected {
            self.draw_arrows(ui, [x, y], flags);
        }
        let pointer = ui.pointer().map(|p| gui.to_gui(p));
        if let Some(pointer) = pointer.filter(|_| flags.hovered) {
            let latency = Rect::new(x + ENTRY_WIDTH - 15.0, y, 10.0, 8.0);
            if latency.contains(pointer) {
                tips.push(self.latency_tip(row));
            } else if info.contains(pointer) {
                tips.extend(self.sample_tips(row));
            }
        }
    }

    fn draw_description(&self, ui: &mut Ui, row: &ServerRow, at: [f32; 2]) {
        let (text, color) = match &row.status {
            RowStatus::Online { motd, .. } => (strip_formatting(motd), DIM_GRAY),
            RowStatus::Unreachable => {
                (self.lang.tr("umicraft.server.unreachable").to_string(), RED)
            }
            RowStatus::Pinging | RowStatus::VersionMismatch => return,
        };
        let width = (ENTRY_WIDTH - 32.0 - 2.0) * self.gui.scale;
        for (i, line) in wrap_lines(self.atlas, &text, width, self.gui.scale)
            .iter()
            .take(2)
            .enumerate()
        {
            let line_at = [at[0], at[1] + i as f32 * LINE_HEIGHT];
            self.gui
                .text(ui, self.atlas, line, TextPlacement::left(line_at, color));
        }
    }

    /// Draws the player count (or the version complaint); returns its box.
    fn draw_info(&self, ui: &mut Ui, row: &ServerRow, origin: [f32; 2]) -> Rect {
        let (text, color) = match &row.status {
            RowStatus::Online { players, max, .. } => (format!("{players}/{max}"), DIM_GRAY),
            RowStatus::VersionMismatch => (
                self.lang.tr("umicraft.server.incompatible").to_string(),
                RED,
            ),
            RowStatus::Pinging | RowStatus::Unreachable => return Rect::ZERO,
        };
        let width = self.atlas.measure(&text, self.gui.scale) / self.gui.scale;
        let x = origin[0] + ENTRY_WIDTH - width - 15.0 - 2.0;
        self.gui.text(
            ui,
            self.atlas,
            &text,
            TextPlacement::left([x, origin[1] + 1.0], color),
        );
        Rect::new(x, origin[1], width, 8.0)
    }

    /// The latency icon of `icons.png` (cell `u = k * 10`, `v = 176 + l * 8`,
    /// 10x8) or five drawn bars without the sheet.
    fn draw_latency(&self, ui: &mut Ui, row: &ServerRow, at: [f32; 2]) {
        let (pinging, level) = match &row.status {
            RowStatus::Pinging => (true, self.ping_phase()),
            RowStatus::Online { latency_ms, .. } => (false, latency_level(*latency_ms)),
            RowStatus::Unreachable | RowStatus::VersionMismatch => (false, 5),
        };
        let rect = self.gui.rect(at[0], at[1], 10.0, 8.0);
        if let Some(sheet) = self.icons {
            let u = if pinging { 10 } else { 0 };
            let src = sheet.sub(u, 176 + u32::from(level) * 8, 10, 8);
            ui.draw().image(src, rect, [1.0; 4]);
            return;
        }
        let lit = if level >= 5 {
            0
        } else {
            5 - usize::from(level)
        };
        for bar in 0..5 {
            let color = if level >= 5 {
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

    /// The bouncing level of the pinging icon (`(time / 100 + index * 2) & 7`
    /// folded to 0 to 4); two ticks are 100 ms.
    fn ping_phase(&self) -> u8 {
        let phase = ((self.ticks / 2) & 7) as u8;
        if phase > 4 {
            8 - phase
        } else {
            phase
        }
    }

    fn draw_arrows(&self, ui: &mut Ui, at: [f32; 2], flags: RowFlags) {
        let gui = self.gui;
        ui.draw()
            .rect(gui.rect(at[0], at[1], 32.0, 32.0), HOVER_SHADE);
        let mark = |ui: &mut Ui, text: &str, x: f32, y: f32| {
            gui.text(
                ui,
                self.atlas,
                text,
                TextPlacement::left([at[0] + x, at[1] + y], WHITE),
            );
        };
        mark(ui, ">", 21.0, 12.0);
        if flags.can_up {
            mark(ui, "^", 5.0, 3.0);
        }
        if flags.can_down {
            mark(ui, "v", 5.0, 21.0);
        }
    }

    fn latency_tip(&self, row: &ServerRow) -> TooltipRow {
        let text = match &row.status {
            RowStatus::Pinging => self.lang.tr("umicraft.server.pinging").to_string(),
            RowStatus::Online { latency_ms, .. } => format!("{latency_ms}ms"),
            RowStatus::Unreachable => self.lang.tr("umicraft.server.noConnection").to_string(),
            RowStatus::VersionMismatch => self.lang.tr("umicraft.server.incompatible").to_string(),
        };
        TooltipRow {
            text,
            rgb: 0xFF_FF_FF,
        }
    }

    fn sample_tips(&self, row: &ServerRow) -> Vec<TooltipRow> {
        let RowStatus::Online {
            version, sample, ..
        } = &row.status
        else {
            return Vec::new();
        };
        let lines = std::iter::once(version)
            .chain(sample)
            .filter(|line| !line.is_empty());
        lines
            .map(|line| TooltipRow {
                text: strip_formatting(line),
                rgb: 0xAA_AA_AA,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_thresholds_are_the_vanilla_ones() {
        let levels: Vec<u8> = [0, 149, 150, 299, 300, 599, 600, 999, 1000]
            .map(latency_level)
            .to_vec();
        assert_eq!(levels, [0, 0, 1, 1, 2, 2, 3, 3, 4]);
    }

    #[test]
    fn zones_follow_the_icon_halves() {
        assert_eq!(zone_at([20.0, 10.0], true, true), RowZone::Play);
        assert_eq!(zone_at([5.0, 5.0], true, true), RowZone::Up);
        assert_eq!(zone_at([5.0, 25.0], true, true), RowZone::Down);
        assert_eq!(zone_at([5.0, 5.0], false, true), RowZone::Body);
        assert_eq!(zone_at([5.0, 25.0], true, false), RowZone::Body);
        assert_eq!(zone_at([100.0, 10.0], true, true), RowZone::Body);
    }
}
