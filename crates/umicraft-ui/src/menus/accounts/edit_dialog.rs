// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Edit dialog: renames an offline account.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::input::Key;
use crate::menus::background::draw_menu_background;
use crate::menus::common::{escape_pressed, key_pressed, Gui};
use crate::text::FontAtlas;

use super::layout::name_dialog_layout;
use super::name_field::NameField;
use super::{AccountsOutcome, DialogEvent};

/// The dialog state: the account being edited and its new name.
#[derive(Debug, Clone)]
pub(super) struct EditDialog {
    index: usize,
    field: NameField,
}

impl EditDialog {
    /// A dialog editing account `index`, prefilled with its `username`.
    pub(super) fn new(index: usize, username: &str) -> Self {
        Self {
            index,
            field: NameField::new(username),
        }
    }

    /// Advances the caret blink by one tick.
    pub(super) fn tick(&mut self) {
        self.field.tick();
    }

    /// Draws one frame; Save needs a valid name.
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
    ) -> Option<DialogEvent> {
        draw_menu_background(ui);
        let gui = Gui::new(ui.viewport());
        let layout = name_dialog_layout(ui.viewport(), false);
        let title = lang.tr("umicraft.accounts.edit.title");
        gui.title(ui, atlas, title, gui.cx(), 30.0);
        ui.push_id("edit-dialog");
        self.field.show(ui, (&gui, atlas, lang), layout.field);
        let enabled = self.field.is_valid();
        let (save_id, cancel_id) = (ui.id("save"), ui.id("cancel"));
        let label = lang.tr("umicraft.accounts.edit.save");
        let save = gui.button(ui, save_id, layout.primary, atlas, label, enabled);
        let label = lang.tr("gui.cancel");
        let cancel = gui.button(ui, cancel_id, layout.cancel, atlas, label, true);
        ui.pop_id();

        if enabled && (save || key_pressed(ui, Key::Enter)) {
            return Some(DialogEvent::Outcome(AccountsOutcome::EditOffline {
                index: self.index,
                username: self.field.text().to_string(),
            }));
        }
        (cancel || escape_pressed(ui)).then_some(DialogEvent::Close)
    }
}
