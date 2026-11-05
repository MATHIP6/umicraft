// SPDX-License-Identifier: MIT OR Apache-2.0

//! The snooper page (`GuiSnooper`, "Snooper Settings...").
//!
//! The client collects and sends no usage data at all, so the list of
//! collected values the reference shows is empty and the switch only stores
//! the choice (default off). The page exists so the hub button opens what
//! vanilla opens.

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, Ui};

use super::layout::{Gui, BUTTON_H, GREY, HALF_W};
use super::{on_off, option_label, OptionsScreen, Outcome};
use crate::options::Options;

/// Margin of the description, `width - 30` wide in the reference.
const DESCRIPTION_MARGIN: f32 = 30.0;
/// Top of the stats list and its distance from the bottom.
const LIST_TOP: f32 = 80.0;
const LIST_BOTTOM_GAP: f32 = 40.0;

impl OptionsScreen {
    /// `GuiSnooper`: title, the wrapped description, an empty list of
    /// collected values, the "Allow snooper" switch and Done.
    pub(super) fn show_snooper(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        let bottom = gui.height - LIST_BOTTOM_GAP;
        Self::list_container(ui, gui, (0.0, gui.width), (LIST_TOP, bottom), 0.0);
        Self::list_overlay(ui, gui, (0.0, gui.width), (LIST_TOP, bottom));
        Self::title(ui, atlas, gui, lang.tr("options.snooper.title"), 8.0);
        let lines = wrap(
            atlas,
            lang.tr("options.snooper.desc"),
            gui.width - DESCRIPTION_MARGIN,
        );
        for (index, line) in lines.iter().enumerate() {
            Self::centered_text(
                ui,
                atlas,
                gui,
                (gui.cx(), 22.0 + 9.0 * index as f32),
                line,
                GREY,
            );
        }

        let cx = gui.cx();
        let toggle = gui.rect(cx - 152.0, gui.height - 30.0, HALF_W, BUTTON_H);
        let label = option_label(
            lang,
            "options.snooper",
            on_off(lang, options.multiplayer.snooper),
        );
        if self.button(ui, atlas, "snooper_toggle", toggle, &label, true, None) {
            options.multiplayer.snooper = !options.multiplayer.snooper;
            outcome.changed = true;
        }
        let done = gui.rect(cx + 2.0, gui.height - 30.0, HALF_W, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }
}

/// Greedy word wrap to `max_width` GUI units
/// (`FontRenderer.listFormattedStringToWidth`).
fn wrap(atlas: &FontAtlas, text: &str, max_width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if atlas.measure(&format!("{line} {word}"), 1.0) <= max_width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_wraps_into_lines_that_fit() {
        let atlas = FontAtlas::fallback();
        let text = "We want to collect information about your computer to help improve the game";
        let lines = wrap(&atlas, text, 100.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|line| atlas.measure(line, 1.0) <= 100.0));
        assert_eq!(lines.join(" "), text);
    }
}
