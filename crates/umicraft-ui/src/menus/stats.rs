// SPDX-License-Identifier: MIT OR Apache-2.0

//! The read-only statistics screen (MCP `GuiStats`, general tab).
//!
//! The title (`gui.stats`) is centred at `y = 20`. The rows fill the area from
//! `y = 32` to `height - 64`, 10 GUI units each, with a darker band behind
//! every other row as in `GuiSlot`; the label is left, the value right inside
//! a 300-unit column. The Done button is at `(cx - 100, height - 28)`.
//! Achievements, the block/item/mob tabs and the server statistics download
//! are out of scope: the caller lists the rows it has.

use umicraft_core::lang::Lang;

use super::background::draw_menu_background;
use crate::context::Ui;
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::common::{draw_scrollbar, escape_pressed, wheel_scrolled, Gui, TextPlacement, WHITE};

/// Height of one row in GUI units (`GuiStats.StatsGeneral` slot height).
const ROW_HEIGHT: f32 = 10.0;
/// Half the width of the row column in GUI units.
const HALF_COLUMN: f32 = 150.0;

/// One line of the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatRow {
    /// The translation key of the label (`stat.walkOneCm`).
    pub key: String,
    /// The formatted value (`1.2 km`).
    pub value: String,
}

impl StatRow {
    /// A row from a label key and its value.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// What the player did on the statistics screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsOutcome {
    /// Leave the screen (Done or Escape).
    Done,
}

/// The scroll state of the statistics screen.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StatsScreen {
    offset: f32,
}

/// The Done button for `viewport`.
#[must_use]
pub fn stats_done_rect(viewport: Rect) -> Rect {
    let gui = Gui::new(viewport);
    gui.rect(gui.cx() - 100.0, gui.height - 28.0, 200.0, 20.0)
}

impl StatsScreen {
    /// Creates the screen scrolled to the top.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The scroll offset in GUI units.
    #[must_use]
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Draws one frame of `rows` and returns the player's choice.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        rows: &[StatRow],
    ) -> Option<StatsOutcome> {
        draw_menu_background(ui);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        gui.title(ui, atlas, lang.tr("gui.stats"), gui.cx(), 20.0);

        let (top, bottom) = (32.0, gui.height - 64.0);
        let view = gui.rect(0.0, top, gui.width, bottom - top);
        let content = rows.len() as f32 * ROW_HEIGHT;
        self.offset = wheel_scrolled(ui, &gui, view, content, self.offset);

        ui.push_clip(view);
        for (index, row) in rows.iter().enumerate() {
            let y = top + index as f32 * ROW_HEIGHT - self.offset;
            if y + ROW_HEIGHT < top || y > bottom {
                continue;
            }
            draw_row(ui, atlas, lang, &gui, row, y, index % 2 == 1);
        }
        ui.pop_clip();
        let bar_x = gui.cx() + HALF_COLUMN + 4.0;
        draw_scrollbar(ui, &gui, [bar_x, top, bottom - top], content, self.offset);

        ui.push_id("stats");
        let done_id = ui.id("done");
        let done = gui.button(
            ui,
            done_id,
            stats_done_rect(viewport),
            atlas,
            lang.tr("gui.done"),
            true,
        );
        ui.pop_id();
        (done || escape_pressed(ui)).then_some(StatsOutcome::Done)
    }
}

/// Draws a row: the shaded band, the label on the left, the value on the right.
fn draw_row(
    ui: &mut Ui,
    atlas: &FontAtlas,
    lang: &Lang,
    gui: &Gui,
    row: &StatRow,
    y: f32,
    shaded: bool,
) {
    let left = gui.cx() - HALF_COLUMN;
    if shaded {
        ui.draw().rect(
            gui.rect(left, y, 2.0 * HALF_COLUMN, ROW_HEIGHT),
            [0.0, 0.0, 0.0, 0.2],
        );
    }
    gui.text(
        ui,
        atlas,
        lang.tr(&row.key),
        TextPlacement::left([left + 2.0, y + 1.0], WHITE),
    );
    let width = atlas.measure(&row.value, gui.scale) / gui.scale;
    let at = [left + 2.0 * HALF_COLUMN - width - 2.0, y + 1.0];
    gui.text(ui, atlas, &row.value, TextPlacement::left(at, WHITE));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, Key, UiInput};
    use crate::menus::common::testing::{click_at, frame, key, VIEWPORT};

    fn rows(count: usize) -> Vec<StatRow> {
        (0..count)
            .map(|i| StatRow::new("stat.startGame", i.to_string()))
            .collect()
    }

    fn run(screen: &mut StatsScreen, input: UiInput, rows: &[StatRow]) -> Option<StatsOutcome> {
        frame(input, |ui| {
            screen.show(ui, &FontAtlas::fallback(), &Lang::builtin(), rows)
        })
    }

    #[test]
    fn done_and_escape_leave() {
        let mut screen = StatsScreen::new();
        let done = stats_done_rect(VIEWPORT);
        // scale 3: (213 - 100) * 3, (240 - 28) * 3
        assert_eq!((done.x, done.y), (339.0, 636.0));
        assert_eq!(
            run(&mut screen, click_at(done), &[]),
            Some(StatsOutcome::Done)
        );
        assert_eq!(
            run(&mut screen, key(Key::Escape), &[]),
            Some(StatsOutcome::Done)
        );
        assert_eq!(run(&mut screen, UiInput::default(), &[]), None);
    }

    #[test]
    fn the_wheel_scrolls_long_lists_only() {
        let wheel = |delta| {
            UiInput::new(vec![
                InputEvent::PointerMoved {
                    position: [640.0, 300.0],
                },
                InputEvent::Wheel { delta },
            ])
        };
        let mut long = StatsScreen::new();
        run(&mut long, wheel(1.0), &rows(100));
        assert_eq!(long.offset(), 18.0);
        let mut short = StatsScreen::new();
        run(&mut short, wheel(1.0), &rows(3));
        assert_eq!(short.offset(), 0.0);
    }
}
