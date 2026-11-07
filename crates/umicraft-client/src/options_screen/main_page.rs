// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hub page (`GuiOptions`).

use umicraft_core::lang::Lang;
use umicraft_ui::menus::TitleStyle;
use umicraft_ui::{FontAtlas, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W};
use super::{option_label, OptionsScreen, Outcome, Page};
use crate::options::{Options, FOV};

impl OptionsScreen {
    /// `GuiOptions`.
    pub(super) fn show_main(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, lang.tr("options.title"), 15.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        let left = cx - 155.0;
        let right = cx + 5.0;

        let fov = options.video.fov;
        let fraction = (fov - FOV.start()) / (FOV.end() - FOV.start());
        let rect = gui.rect(left, h6 - 12.0, HALF_W, BUTTON_H);
        if let Some(value) = self.slider(ui, atlas, "fov", rect, &fov_label(lang, fov), fraction) {
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
            &option_label(
                lang,
                "options.difficulty",
                lang.tr("options.difficulty.normal"),
            ),
            false,
            None,
        );

        // Umicraft's own option, in the free row between the sliders and the pages.
        let style = gui.rect(left, h6 + 18.0, 2.0 * HALF_W, BUTTON_H);
        let label = option_label(
            lang,
            "umicraft.options.titleStyle",
            title_style_text(lang, options),
        );
        if self.button(ui, atlas, "title_style", style, &label, true, None) {
            options.menu.toggle_title_style();
            outcome.changed = true;
        }

        // (id, label key, y offset, x): the pages the client has no backend
        // for (shaders, stream) are drawn and inert.
        let entries: [(&str, &str, f32, f32); 10] = [
            ("skin", "options.skinCustomisation", 48.0, left),
            ("shaders", "umicraft.options.shaders", 48.0, right),
            ("sounds", "options.sounds", 72.0, left),
            ("stream", "options.stream", 72.0, right),
            ("nav:video", "options.video", 96.0, left),
            ("nav:controls", "options.controls", 96.0, right),
            ("language", "options.language", 120.0, left),
            ("nav:chat", "options.chat.title", 120.0, right),
            ("resourcepacks", "options.resourcepack", 144.0, left),
            ("snooper", "options.snooper.view", 144.0, right),
        ];
        for (id, key, row, x) in entries {
            let rect = gui.rect(x, h6 + row - 6.0, HALF_W, BUTTON_H);
            if self.button(ui, atlas, id, rect, lang.tr(key), true, None) {
                let page = match id {
                    "nav:video" => Page::Video,
                    "nav:controls" => Page::Controls,
                    "nav:chat" => Page::Chat,
                    "skin" => Page::Skin,
                    "sounds" => Page::Sounds,
                    "language" => Page::Language,
                    "resourcepacks" => Page::ResourcePacks,
                    "snooper" => Page::Snooper,
                    // Vanilla opens a screen the client has no backend for
                    // (shaders, Twitch stream).
                    _ => continue,
                };
                self.open(page, options);
            }
        }

        let done = gui.rect(cx - 100.0, h6 + 168.0, 200.0, BUTTON_H);
        if self.button(ui, atlas, "done", done, lang.tr("gui.done"), true, None) {
            outcome.done = true;
        }
    }
}

/// The translated name of the current title screen style.
pub(super) fn title_style_text<'a>(lang: &'a Lang, options: &Options) -> &'a str {
    lang.tr(match options.menu.style() {
        TitleStyle::Modern => "umicraft.options.titleStyle.modern",
        TitleStyle::Classic => "umicraft.options.titleStyle.classic",
    })
}

/// `GameSettings.getKeyBinding(FOV)`.
pub(super) fn fov_label(lang: &Lang, fov: f32) -> String {
    let value = if fov == 70.0 {
        lang.tr("options.fov.min").to_string()
    } else if fov == 110.0 {
        lang.tr("options.fov.max").to_string()
    } else {
        (fov as i32).to_string()
    };
    option_label(lang, "options.fov", &value)
}
