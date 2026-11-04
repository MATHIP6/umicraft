// SPDX-License-Identifier: MIT OR Apache-2.0

//! The disconnect screen (MCP `GuiDisconnected`).
//!
//! The title (grey, `0xAAAAAA`) is centred above the reason, which is wrapped
//! to `width - 50` units and drawn white, one line per 9 units, centred on
//! `height / 2`. The button (`gui.toMenu`) sits below the text at
//! `(cx - 100, height / 2 + textHeight / 2 + 9)`.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::game_menu::draw_default_background;
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::common::{
    escape_pressed, strip_formatting, wrap_lines, Gui, TextPlacement, LIGHT_GRAY, LINE_HEIGHT,
    WHITE,
};

/// What the player did on the disconnect screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectedOutcome {
    /// Return to the server list (the button or Escape).
    BackToList,
}

/// Why the connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisconnectedScreen {
    title_key: String,
    reason: String,
}

impl DisconnectedScreen {
    /// A screen with a title translation key (`disconnect.lost`,
    /// `connect.failed`, ...) and the reason text, which may hold `\n` and
    /// legacy `§` codes (stripped: the menu draws plain text).
    #[must_use]
    pub fn new(title_key: impl Into<String>, reason: &str) -> Self {
        Self {
            title_key: title_key.into(),
            reason: strip_formatting(reason),
        }
    }

    /// The reason as drawn, without formatting codes.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The rectangle of the button for a reason of `line_count` lines.
    #[must_use]
    pub fn button_rect(viewport: Rect, line_count: usize) -> Rect {
        let gui = Gui::new(viewport);
        let text_height = (line_count as f32 * LINE_HEIGHT).floor();
        let y = (gui.height / 2.0).floor() + (text_height / 2.0).floor() + LINE_HEIGHT;
        gui.rect(gui.cx() - 100.0, y, 200.0, 20.0)
    }

    /// Draws one frame and returns the player's choice.
    pub fn show(&self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<DisconnectedOutcome> {
        draw_default_background(ui);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let lines = wrap_lines(
            atlas,
            &self.reason,
            (gui.width - 50.0) * gui.scale,
            gui.scale,
        );
        let text_height = (lines.len() as f32 * LINE_HEIGHT).floor();
        let top = (gui.height / 2.0).floor() - (text_height / 2.0).floor();

        let title_at = [gui.cx(), top - LINE_HEIGHT * 2.0];
        let title = lang.tr(&self.title_key);
        gui.text(
            ui,
            atlas,
            title,
            TextPlacement::centered(title_at, LIGHT_GRAY),
        );
        for (index, line) in lines.iter().enumerate() {
            let at = [gui.cx(), top + index as f32 * LINE_HEIGHT];
            gui.text(ui, atlas, line, TextPlacement::centered(at, WHITE));
        }

        ui.push_id("disconnected");
        let id = ui.id("back");
        let rect = Self::button_rect(viewport, lines.len());
        let back = gui.button(ui, id, rect, atlas, lang.tr("gui.toMenu"), true);
        ui.pop_id();
        (back || escape_pressed(ui)).then_some(DisconnectedOutcome::BackToList)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, UiInput};
    use crate::menus::common::testing::{click_at, frame, key, VIEWPORT};

    fn run(screen: &DisconnectedScreen, input: UiInput) -> Option<DisconnectedOutcome> {
        frame(input, |ui| {
            screen.show(ui, &FontAtlas::fallback(), &Lang::builtin())
        })
    }

    #[test]
    fn the_button_returns_to_the_list() {
        let screen = DisconnectedScreen::new("disconnect.lost", "Timed out");
        let rect = DisconnectedScreen::button_rect(VIEWPORT, 1);
        assert_eq!(
            run(&screen, click_at(rect)),
            Some(DisconnectedOutcome::BackToList)
        );
        assert_eq!(
            run(&screen, key(Key::Escape)),
            Some(DisconnectedOutcome::BackToList)
        );
        assert_eq!(run(&screen, UiInput::default()), None);
    }

    #[test]
    fn the_button_moves_down_with_more_lines() {
        let one = DisconnectedScreen::button_rect(VIEWPORT, 1);
        let four = DisconnectedScreen::button_rect(VIEWPORT, 4);
        assert!(four.y > one.y);
        // 1 line: 120 + 4 + 9 = 133 units * 3
        assert_eq!(one.y, 399.0);
    }

    #[test]
    fn formatting_codes_are_dropped_from_the_reason() {
        let screen = DisconnectedScreen::new("k", "\u{a7}cBanned\nfor cheating");
        assert_eq!(screen.reason(), "Banned\nfor cheating");
    }
}
