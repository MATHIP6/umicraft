// SPDX-License-Identifier: MIT OR Apache-2.0

//! The theme page: one button per theme, the active one marked (ADR-0078).

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, ThemeId, Ui};

use super::layout::{Gui, BUTTON_H};
use super::{OptionsScreen, Outcome};
use crate::options::Options;
use crate::options::ThemeSetting;

/// Width of a theme button in GUI units.
const THEME_BUTTON_W: f32 = 200.0;
/// Distance between two theme buttons, buttons included.
const ROW_STEP: f32 = 24.0;

impl OptionsScreen {
    /// The theme picker: title, a button per theme and Done. A pick applies at
    /// once, since the next frame is already painted with it.
    pub(super) fn show_theme(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        Self::title(
            ui,
            atlas,
            gui,
            lang.tr("umicraft.options.theme.title"),
            15.0,
        );
        let cx = gui.cx();
        let top = (gui.height / 6.0).floor();
        for (row, id) in ThemeId::ALL.into_iter().enumerate() {
            let rect = gui.rect(
                cx - THEME_BUTTON_W / 2.0,
                top + ROW_STEP * row as f32,
                THEME_BUTTON_W,
                BUTTON_H,
            );
            let label = theme_label(lang, id, options.menu.theme() == id);
            if self.button(ui, atlas, id.lang_key(), rect, &label, true, None) {
                options.menu.theme = ThemeSetting::from(id);
                outcome.changed = true;
            }
        }
        let done = gui.rect(cx - 100.0, gui.height / 6.0 + 168.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }
}

/// The translated theme name, framed by arrows when it is the active one.
fn theme_label(lang: &Lang, id: ThemeId, active: bool) -> String {
    let name = lang.tr(id.lang_key());
    if active {
        format!("> {name} <")
    } else {
        name.to_string()
    }
}
