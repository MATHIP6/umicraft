// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Add account dialog: an offline name or a Microsoft login.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::input::Key;
use crate::menus::background::draw_menu_background;
use crate::menus::common::{escape_pressed, key_pressed, Gui};
use crate::text::FontAtlas;

use super::layout::name_dialog_layout;
use super::name_field::NameField;
use super::{AccountsOutcome, DialogEvent};

/// The dialog state: the name being typed.
#[derive(Debug, Clone)]
pub(super) struct AddDialog {
    field: NameField,
}

impl AddDialog {
    /// An empty dialog.
    pub(super) fn new() -> Self {
        Self {
            field: NameField::new(""),
        }
    }

    /// Advances the caret blink by one tick.
    pub(super) fn tick(&mut self) {
        self.field.tick();
    }

    /// Draws one frame; the offline button needs a valid name.
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
    ) -> Option<DialogEvent> {
        draw_menu_background(ui);
        let gui = Gui::new(ui.viewport());
        let layout = name_dialog_layout(ui.viewport(), true);
        let title = lang.tr("umicraft.accounts.add.title");
        gui.title(ui, atlas, title, gui.cx(), 30.0);
        ui.push_id("add-dialog");
        self.field.show(ui, (&gui, atlas, lang), layout.field);
        let enabled = self.field.is_valid();
        let ids = [ui.id("offline"), ui.id("microsoft"), ui.id("cancel")];
        let label = lang.tr("umicraft.accounts.add.offline");
        let offline = gui.button(ui, ids[0], layout.primary, atlas, label, enabled);
        let label = lang.tr("umicraft.accounts.add.microsoft");
        let microsoft = gui.button(ui, ids[1], layout.microsoft, atlas, label, true);
        let label = lang.tr("gui.cancel");
        let cancel = gui.button(ui, ids[2], layout.cancel, atlas, label, true);
        ui.pop_id();

        if enabled && (offline || key_pressed(ui, Key::Enter)) {
            let name = self.field.text().to_string();
            return Some(DialogEvent::Outcome(AccountsOutcome::AddOffline(name)));
        }
        if microsoft {
            return Some(DialogEvent::Outcome(AccountsOutcome::AddMicrosoft));
        }
        (cancel || escape_pressed(ui)).then_some(DialogEvent::Close)
    }
}
