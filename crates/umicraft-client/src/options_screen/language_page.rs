// SPDX-License-Identifier: MIT OR Apache-2.0

//! The language page (`GuiLanguage`).
//!
//! A click on a language selects it at once and raises
//! [`Outcome::language_changed`]; the window then rebuilds the `Lang` handle
//! (vanilla refreshes its resources in `List.elementClicked` the same way).
//! "Force Unicode Font" is not offered: the client has one font.

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, Ui};

use super::layout::{Gui, BUTTON_H, GREY, HALF_W};
use super::{OptionsScreen, Outcome};
use crate::options::Options;

/// Height of one row of the list (`GuiLanguage.List`).
const ROW: f32 = 18.0;
/// Width of the selection box (`GuiSlot.getListWidth`).
const LIST_WIDTH: f32 = 220.0;

impl OptionsScreen {
    /// `GuiLanguage`.
    pub(super) fn show_language(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (32.0, gui.height - 61.0);
        let cx = gui.cx();
        let content = self.catalog.languages().len() as f32 * ROW;
        self.scroll_list(ui, content, bottom - top, ROW);
        Self::list_container(ui, gui, (0.0, gui.width), (top, bottom), self.scroll);

        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        let mut picked = None;
        for (index, language) in self.catalog.languages().iter().enumerate() {
            let y = top + 4.0 - self.scroll + index as f32 * ROW;
            let rect = gui.rect(cx - LIST_WIDTH / 2.0, y - 2.0, LIST_WIDTH, ROW);
            if language.code == options.language.code {
                Self::selection_box(ui, gui, cx, y);
            }
            let label = format!("{} ({})", language.name, language.region);
            Self::centered_text(ui, atlas, gui, (cx, y + 1.0), &label, [1.0; 4]);
            let id = format!("language:{}", language.code);
            self.rects.insert(id.clone(), rect);
            let widget = ui.id(&id);
            if ui.response(widget, rect).clicked {
                picked = Some(language.code.clone());
            }
        }
        ui.pop_clip();
        Self::list_overlay(ui, gui, (0.0, gui.width), (top, bottom));
        Self::scrollbar(ui, gui, cx + 124.0, (top, bottom), content, self.scroll);
        if let Some(code) = picked.filter(|code| *code != options.language.code) {
            options.language.code = code;
            outcome.changed = true;
            outcome.language_changed = true;
        }

        Self::title(ui, atlas, gui, lang.tr("options.language"), 16.0);
        let warning = format!("({})", lang.tr("options.languageWarning"));
        Self::centered_text(ui, atlas, gui, (cx, gui.height - 56.0), &warning, GREY);
        let done = gui.rect(cx - 155.0 + 160.0, gui.height - 38.0, HALF_W, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }

    /// The box `GuiSlot` draws around the selected row: a grey border with a
    /// black inside.
    fn selection_box(ui: &mut Ui, gui: Gui, cx: f32, y: f32) {
        let left = cx - LIST_WIDTH / 2.0;
        ui.draw().rect(
            gui.rect(left, y - 2.0, LIST_WIDTH, ROW),
            [0.5, 0.5, 0.5, 1.0],
        );
        ui.draw().rect(
            gui.rect(left + 1.0, y - 1.0, LIST_WIDTH - 2.0, ROW - 2.0),
            [0.0, 0.0, 0.0, 1.0],
        );
    }
}
