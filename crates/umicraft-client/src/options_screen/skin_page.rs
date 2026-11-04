// SPDX-License-Identifier: MIT OR Apache-2.0

//! The skin customisation page (`GuiCustomizeSkin`).

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W};
use super::{on_off, option_label, OptionsScreen, Outcome};
use crate::options::{Options, SkinPart};

impl OptionsScreen {
    /// `GuiCustomizeSkin`: one on/off button per model part, two per row,
    /// and Done on the row after the last one.
    pub(super) fn show_skin(
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
            lang.tr("options.skinCustomisation.title"),
            20.0,
        );
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        for (index, part) in SkinPart::ALL.into_iter().enumerate() {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let rect = gui.rect(x, h6 + 24.0 * (index / 2) as f32, HALF_W, BUTTON_H);
            let label = option_label(
                lang,
                part.lang_key(),
                on_off(lang, options.skin.is_enabled(part)),
            );
            if self.button(
                ui,
                atlas,
                &format!("part:{}", part.id()),
                rect,
                &label,
                true,
                None,
            ) {
                options.skin.toggle(part);
                outcome.changed = true;
                outcome.skin_changed = true;
            }
        }
        // `if (i % 2 == 1) ++i`: Done goes under the last full row.
        let rows = SkinPart::ALL.len().div_ceil(2);
        let done = gui.rect(cx - 100.0, h6 + 24.0 * rows as f32, 200.0, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }
}
