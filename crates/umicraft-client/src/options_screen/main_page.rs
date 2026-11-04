// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hub page (`GuiOptions`).

use umicraft_ui::{FontAtlas, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W};
use super::{OptionsScreen, Outcome, Page};
use crate::options::{Options, FOV};

impl OptionsScreen {
    /// `GuiOptions`.
    pub(super) fn show_main(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, "Options", 15.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        let left = cx - 155.0;
        let right = cx + 5.0;

        let fov = options.video.fov;
        let fraction = (fov - FOV.start()) / (FOV.end() - FOV.start());
        let rect = gui.rect(left, h6 - 12.0, HALF_W, BUTTON_H);
        if let Some(value) = self.slider(ui, atlas, "fov", rect, &fov_label(fov), fraction) {
            options.video.fov = (FOV.start() + value * (FOV.end() - FOV.start())).round();
            outcome.changed = true;
        }
        // The difficulty belongs to the server (`isSingleplayer()` is false).
        let difficulty = gui.rect(right, h6 - 12.0, HALF_W, BUTTON_H);
        self.button(
            ui,
            atlas,
            "difficulty",
            difficulty,
            "Difficulty: Normal",
            false,
            None,
        );

        // (id, label, y offset, x, enabled): the disabled ones wait for
        // a backend of their own.
        let entries: [(&str, &str, f32, f32, bool); 10] = [
            ("skin", "Skin Customization...", 48.0, left, true),
            ("shaders", "Super Secret Settings...", 48.0, right, true),
            ("sounds", "Music & Sounds...", 72.0, left, true),
            ("stream", "Broadcast Settings...", 72.0, right, true),
            ("nav:video", "Video Settings...", 96.0, left, true),
            ("nav:controls", "Controls...", 96.0, right, true),
            ("language", "Language...", 120.0, left, true),
            ("nav:chat", "Chat Settings...", 120.0, right, true),
            ("resourcepacks", "Resource Packs...", 144.0, left, true),
            ("snooper", "Snooper Settings...", 144.0, right, true),
        ];
        for (id, label, row, x, enabled) in entries {
            let rect = gui.rect(x, h6 + row - 6.0, HALF_W, BUTTON_H);
            if self.button(ui, atlas, id, rect, label, enabled, None) {
                let page = match id {
                    "nav:video" => Page::Video,
                    "nav:controls" => Page::Controls,
                    "nav:chat" => Page::Chat,
                    // Vanilla opens a screen the client does not have yet.
                    _ => continue,
                };
                self.page = page;
                self.scroll = 0.0;
            }
        }

        let done = gui.rect(cx - 100.0, h6 + 168.0, 200.0, BUTTON_H);
        if self.button(ui, atlas, "done", done, "Done", true, None) {
            outcome.done = true;
        }
    }
}

/// `GameSettings.getKeyBinding(FOV)`.
pub(super) fn fov_label(fov: f32) -> String {
    if fov == 70.0 {
        "FOV: Normal".to_string()
    } else if fov == 110.0 {
        "FOV: Quake Pro".to_string()
    } else {
        format!("FOV: {}", fov as i32)
    }
}
