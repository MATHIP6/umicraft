// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sound volume page (`GuiScreenOptionsSounds`).
//!
//! The sliders change `options.sound`; the window pushes the volumes to the
//! audio thread (`audio_bridge::settings`).

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W};
use super::{option_label, OptionsScreen, Outcome};
use crate::options::{CategoryLabel, Options, SoundCategory};

/// Width of the master volume slider (`GuiScreenOptionsSounds.Button`).
const MASTER_W: f32 = 310.0;

impl OptionsScreen {
    /// `GuiScreenOptionsSounds`: the master volume across the whole width,
    /// then the other categories two per row.
    pub(super) fn show_sounds(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, lang.tr("options.sounds.title"), 15.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        // Vanilla advances the slot counter by two after the master slider,
        // so the other categories start on the second row.
        for (slot, category) in SoundCategory::ALL.into_iter().enumerate() {
            let (index, width) = match category {
                SoundCategory::Master => (0, MASTER_W),
                _ => (slot + 1, HALF_W),
            };
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = h6 - 12.0 + 24.0 * (index / 2) as f32;
            let rect = gui.rect(x, y, width, BUTTON_H);
            let volume = options.sound.volume(category);
            let label = option_label(lang, category.lang_key(), &volume_text(lang, volume));
            let id = format!("sound:{}", category.lang_key());
            if let Some(value) = self.slider(ui, atlas, &id, rect, &label, volume) {
                options.sound.set_volume(category, value);
                outcome.changed = true;
            }
        }
        let done = gui.rect(cx - 100.0, h6 + 168.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }
}

/// `GuiScreenOptionsSounds.getSoundVolume`: "OFF" at zero, else a percentage.
pub(super) fn volume_text(lang: &Lang, volume: f32) -> String {
    if volume == 0.0 {
        lang.tr("options.off").to_string()
    } else {
        format!("{}%", (volume * 100.0) as i32)
    }
}
