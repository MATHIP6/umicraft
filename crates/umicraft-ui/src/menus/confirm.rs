// SPDX-License-Identifier: MIT OR Apache-2.0

//! The yes/no dialog (MCP `GuiYesNo`), used to confirm deleting a server.
//!
//! Layout in GUI units: the title is centred at `y = 70`, the message lines
//! (wrapped to `width - 50`) follow from `y = 90` every 9 units, and the two
//! 150x20 buttons sit at `(cx - 155, height / 6 + 96)` and
//! `(cx + 5, height / 6 + 96)`.

use umicraft_core::lang::Lang;

use super::background::draw_menu_background;
use crate::context::Ui;
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::common::{escape_pressed, wrap_lines, Gui, TextPlacement, LINE_HEIGHT, WHITE};

/// What the player answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// The first button.
    Yes,
    /// The second button, or Escape.
    No,
}

/// The rectangles of the two answer buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfirmLayout {
    /// The "yes" button.
    pub yes: Rect,
    /// The "no" button.
    pub no: Rect,
}

/// Computes the vanilla button positions for `viewport`.
#[must_use]
pub fn confirm_layout(viewport: Rect) -> ConfirmLayout {
    let gui = Gui::new(viewport);
    let (cx, y) = (gui.cx(), gui.h_div(6.0) + 96.0);
    ConfirmLayout {
        yes: gui.rect(cx - 155.0, y, 150.0, 20.0),
        no: gui.rect(cx - 155.0 + 160.0, y, 150.0, 20.0),
    }
}

/// A yes/no question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmScreen {
    title_key: String,
    message: String,
    yes_key: String,
    no_key: String,
}

impl ConfirmScreen {
    /// A dialog whose title and button labels are translation keys.
    ///
    /// `message` is drawn as given: it often embeds data (a server name) and
    /// is built by the caller from translated parts.
    #[must_use]
    pub fn new(
        title_key: impl Into<String>,
        message: impl Into<String>,
        yes_key: impl Into<String>,
        no_key: impl Into<String>,
    ) -> Self {
        Self {
            title_key: title_key.into(),
            message: message.into(),
            yes_key: yes_key.into(),
            no_key: no_key.into(),
        }
    }

    /// The vanilla "remove this server?" dialog
    /// (`GuiMultiplayer.actionPerformed`, button 2).
    #[must_use]
    pub fn delete_server(name: &str, lang: &Lang) -> Self {
        let message = format!("'{name}' {}", lang.tr("selectServer.deleteWarning"));
        Self::new(
            "selectServer.deleteQuestion",
            message,
            "selectServer.deleteButton",
            "gui.cancel",
        )
    }

    /// Draws one frame and returns the answer, if any.
    pub fn show(&self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<ConfirmOutcome> {
        draw_menu_background(ui);
        let gui = Gui::new(ui.viewport());
        let layout = confirm_layout(ui.viewport());
        gui.title(ui, atlas, lang.tr(&self.title_key), gui.cx(), 70.0);
        let width = (gui.width - 50.0) * gui.scale;
        let lines = wrap_lines(atlas, &self.message, width, gui.scale);
        for (index, line) in lines.iter().enumerate() {
            let at = [gui.cx(), 90.0 + index as f32 * LINE_HEIGHT];
            gui.text(ui, atlas, line, TextPlacement::centered(at, WHITE));
        }

        ui.push_id("confirm");
        let (yes_id, no_id) = (ui.id("yes"), ui.id("no"));
        let yes = gui.button(ui, yes_id, layout.yes, atlas, lang.tr(&self.yes_key), true);
        let no = gui.button(ui, no_id, layout.no, atlas, lang.tr(&self.no_key), true);
        ui.pop_id();

        if yes {
            Some(ConfirmOutcome::Yes)
        } else if no || escape_pressed(ui) {
            Some(ConfirmOutcome::No)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, UiInput};
    use crate::menus::common::testing::{click_at, frame, key, VIEWPORT};

    fn answer(input: UiInput) -> Option<ConfirmOutcome> {
        let screen = ConfirmScreen::new("t", "message", "gui.yes", "gui.no");
        frame(input, |ui| {
            screen.show(ui, &FontAtlas::fallback(), &Lang::builtin())
        })
    }

    #[test]
    fn the_buttons_answer() {
        let layout = confirm_layout(VIEWPORT);
        assert_eq!(answer(click_at(layout.yes)), Some(ConfirmOutcome::Yes));
        assert_eq!(answer(click_at(layout.no)), Some(ConfirmOutcome::No));
        assert_eq!(answer(key(Key::Escape)), Some(ConfirmOutcome::No));
        assert_eq!(answer(UiInput::default()), None);
    }

    #[test]
    fn the_layout_is_the_vanilla_one() {
        // scale 3, cx = 213, height / 6 = 40
        let layout = confirm_layout(VIEWPORT);
        assert_eq!(layout.yes, Rect::new(174.0, 408.0, 450.0, 60.0));
        assert_eq!(layout.no.x, 654.0);
    }

    #[test]
    fn the_delete_dialog_quotes_the_server_name() {
        let screen = ConfirmScreen::delete_server("Home", &Lang::builtin());
        assert!(screen.message.starts_with("'Home' "));
        assert_eq!(screen.title_key, "selectServer.deleteQuestion");
    }
}
