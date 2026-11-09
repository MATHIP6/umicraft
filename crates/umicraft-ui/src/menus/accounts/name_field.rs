// SPDX-License-Identifier: MIT OR Apache-2.0

//! The username field shared by the Add and Edit dialogs.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::menus::common::{Gui, TextPlacement, LABEL_GRAY};
use crate::text::FontAtlas;
use crate::widgets::{text_field_scaled, TextFieldState};

/// The longest offline username, as in the game.
pub const MAX_USERNAME_CHARS: usize = 16;
/// The red of the validation hint (`§c`).
const ERROR_RED: Color = [1.0, 0.333, 0.333, 1.0];

/// Whether `name` is a valid offline username: 1 to 16 characters from
/// `A-Z a-z 0-9 _`, the set the game accepts for player names.
///
/// ```
/// use umicraft_ui::menus::accounts::is_valid_username;
/// assert!(is_valid_username("Steve_01"));
/// assert!(!is_valid_username("bad name"));
/// assert!(!is_valid_username(""));
/// ```
#[must_use]
pub fn is_valid_username(name: &str) -> bool {
    (1..=MAX_USERNAME_CHARS).contains(&name.chars().count())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A text field with live validation.
#[derive(Debug, Clone)]
pub(super) struct NameField {
    state: TextFieldState,
    focused: bool,
}

impl NameField {
    /// A field prefilled with `text`.
    pub(super) fn new(text: &str) -> Self {
        Self {
            state: TextFieldState::with_text(text),
            focused: false,
        }
    }

    /// The text typed so far.
    pub(super) fn text(&self) -> &str {
        self.state.text()
    }

    /// Whether the text is a valid username.
    pub(super) fn is_valid(&self) -> bool {
        is_valid_username(self.state.text())
    }

    /// Advances the caret blink by one tick.
    pub(super) fn tick(&mut self) {
        self.state.tick();
    }

    /// Draws the label, the field (focused on the first frame) and, below it,
    /// the hint when the text is not empty but invalid.
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        (gui, atlas, lang): (&Gui, &FontAtlas, &Lang),
        field: Rect,
    ) {
        let id = ui.id("name");
        if !self.focused {
            self.focused = true;
            ui.set_focus(Some(id));
        }
        let x = gui.cx() - 100.0;
        let label = TextPlacement::left([x, 64.0], LABEL_GRAY);
        gui.text(ui, atlas, lang.tr("umicraft.accounts.add.username"), label);
        text_field_scaled(ui, id, field, atlas, &mut self.state, gui.scale);
        if !self.state.is_empty() && !self.is_valid() {
            let hint = TextPlacement::left([x, 100.0], ERROR_RED);
            gui.text(ui, atlas, lang.tr("umicraft.accounts.add.invalid"), hint);
        }
    }
}
