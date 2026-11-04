// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat settings page (`ScreenChatOptions`).

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, Rect, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W};
use super::{on_off, option_label, OptionsScreen, Outcome};
use crate::options::{ChatVisibility, Options};

impl OptionsScreen {
    /// A `GuiOptionButton` of an on/off option: "Name: ON" (`options.on` / `options.off`).
    #[allow(clippy::too_many_arguments)]
    fn toggle(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        lang: &Lang,
        name_key: &str,
        value: &mut bool,
        outcome: &mut Outcome,
    ) {
        let label = option_label(lang, name_key, on_off(lang, *value));
        if self.button(ui, atlas, id, rect, &label, true, None) {
            *value = !*value;
            outcome.changed = true;
        }
    }

    /// A `GuiOptionSlider` of an option stored as a fraction of `0.0..=1.0`.
    #[allow(clippy::too_many_arguments)]
    fn fraction(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        label: &str,
        value: &mut f32,
        outcome: &mut Outcome,
    ) {
        if let Some(new) = self.slider(ui, atlas, id, rect, label, *value) {
            *value = new;
            outcome.changed = true;
        }
    }

    /// `ScreenChatOptions`.
    pub(super) fn show_chat(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, lang.tr("options.chat.title"), 20.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        for index in 0..10 {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = h6 + 24.0 * (index / 2) as f32;
            let rect = gui.rect(x, y, HALF_W, BUTTON_H);
            let chat = &mut options.chat;
            match index {
                0 => {
                    let value = lang.tr(match chat.visibility {
                        ChatVisibility::Full => "options.chat.visibility.full",
                        ChatVisibility::Commands => "options.chat.visibility.system",
                        ChatVisibility::Hidden => "options.chat.visibility.hidden",
                    });
                    let label = option_label(lang, "options.chat.visibility", value);
                    if self.button(ui, atlas, "visibility", rect, &label, true, None) {
                        chat.visibility = chat.visibility.next();
                        outcome.changed = true;
                    }
                }
                1 => self.toggle(
                    ui,
                    atlas,
                    "colors",
                    rect,
                    lang,
                    "options.chat.color",
                    &mut chat.colors,
                    outcome,
                ),
                2 => self.toggle(
                    ui,
                    atlas,
                    "links",
                    rect,
                    lang,
                    "options.chat.links",
                    &mut chat.links,
                    outcome,
                ),
                3 => {
                    let label = option_label(
                        lang,
                        "options.chat.opacity",
                        &format!("{}%", (chat.opacity * 90.0 + 10.0) as i32),
                    );
                    self.fraction(
                        ui,
                        atlas,
                        "opacity",
                        rect,
                        &label,
                        &mut chat.opacity,
                        outcome,
                    );
                }
                4 => self.toggle(
                    ui,
                    atlas,
                    "links_prompt",
                    rect,
                    lang,
                    "options.chat.links.prompt",
                    &mut chat.links_prompt,
                    outcome,
                ),
                5 => {
                    let value = if chat.scale == 0.0 {
                        lang.tr("options.off").to_string()
                    } else {
                        format!("{}%", (chat.scale * 100.0) as i32)
                    };
                    let label = option_label(lang, "options.chat.scale", &value);
                    self.fraction(ui, atlas, "scale", rect, &label, &mut chat.scale, outcome);
                }
                6 => {
                    let label = option_label(
                        lang,
                        "options.chat.height.focused",
                        &format!("{}px", (chat.height_focused * 160.0 + 20.0) as i32),
                    );
                    let value = &mut chat.height_focused;
                    self.fraction(ui, atlas, "height_focused", rect, &label, value, outcome);
                }
                7 => {
                    let label = option_label(
                        lang,
                        "options.chat.height.unfocused",
                        &format!("{}px", (chat.height_unfocused * 160.0 + 20.0) as i32),
                    );
                    let value = &mut chat.height_unfocused;
                    self.fraction(ui, atlas, "height_unfocused", rect, &label, value, outcome);
                }
                8 => {
                    let label = option_label(
                        lang,
                        "options.chat.width",
                        &format!("{}px", (chat.width * 280.0 + 40.0) as i32),
                    );
                    self.fraction(ui, atlas, "width", rect, &label, &mut chat.width, outcome);
                }
                _ => self.toggle(
                    ui,
                    atlas,
                    "reduced_debug",
                    rect,
                    lang,
                    "options.reducedDebugInfo",
                    &mut chat.reduced_debug,
                    outcome,
                ),
            }
        }
        let done = gui.rect(cx - 100.0, h6 + 120.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }
}
