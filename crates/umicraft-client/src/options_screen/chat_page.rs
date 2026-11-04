// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat settings page (`ScreenChatOptions`).

use umicraft_ui::{FontAtlas, Rect, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W};
use super::{OptionsScreen, Outcome};
use crate::options::{ChatVisibility, Options};

impl OptionsScreen {
    /// A `GuiOptionButton` of an on/off option: "Name: ON".
    #[allow(clippy::too_many_arguments)]
    fn toggle(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        name: &str,
        value: &mut bool,
        outcome: &mut Outcome,
    ) {
        let label = format!("{name}: {}", if *value { "ON" } else { "OFF" });
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
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, "Chat Settings", 20.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        for index in 0..10 {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = h6 + 24.0 * (index / 2) as f32;
            let rect = gui.rect(x, y, HALF_W, BUTTON_H);
            let chat = &mut options.chat;
            match index {
                0 => {
                    let label = format!(
                        "Chat: {}",
                        match chat.visibility {
                            ChatVisibility::Full => "Shown",
                            ChatVisibility::Commands => "Commands Only",
                            ChatVisibility::Hidden => "Hidden",
                        }
                    );
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
                    "Colors",
                    &mut chat.colors,
                    outcome,
                ),
                2 => self.toggle(
                    ui,
                    atlas,
                    "links",
                    rect,
                    "Web Links",
                    &mut chat.links,
                    outcome,
                ),
                3 => {
                    let label = format!("Opacity: {}%", (chat.opacity * 90.0 + 10.0) as i32);
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
                    "Prompt on Links",
                    &mut chat.links_prompt,
                    outcome,
                ),
                5 => {
                    let label = if chat.scale == 0.0 {
                        "Scale: OFF".to_string()
                    } else {
                        format!("Scale: {}%", (chat.scale * 100.0) as i32)
                    };
                    self.fraction(ui, atlas, "scale", rect, &label, &mut chat.scale, outcome);
                }
                6 => {
                    let label = format!(
                        "Focused Height: {}px",
                        (chat.height_focused * 160.0 + 20.0) as i32
                    );
                    let value = &mut chat.height_focused;
                    self.fraction(ui, atlas, "height_focused", rect, &label, value, outcome);
                }
                7 => {
                    let label = format!(
                        "Unfocused Height: {}px",
                        (chat.height_unfocused * 160.0 + 20.0) as i32
                    );
                    let value = &mut chat.height_unfocused;
                    self.fraction(ui, atlas, "height_unfocused", rect, &label, value, outcome);
                }
                8 => {
                    let label = format!("Width: {}px", (chat.width * 280.0 + 40.0) as i32);
                    self.fraction(ui, atlas, "width", rect, &label, &mut chat.width, outcome);
                }
                _ => self.toggle(
                    ui,
                    atlas,
                    "reduced_debug",
                    rect,
                    "Reduced Debug Info",
                    &mut chat.reduced_debug,
                    outcome,
                ),
            }
        }
        let done = gui.rect(cx - 100.0, h6 + 120.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done);
    }
}
