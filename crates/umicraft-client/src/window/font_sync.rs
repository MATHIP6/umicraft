// SPDX-License-Identifier: MIT OR Apache-2.0

//! Keeps the renderer's UI font in step with the theme and the GUI scale.

use std::sync::Arc;

use umicraft_ui::gui_scale;

use super::App;
use crate::theme_font::{self, FontChoice};
use crate::ui_font;

impl App {
    /// Installs the font the active theme wants at the current GUI scale.
    ///
    /// A swap uploads one atlas, so it only happens when the theme or the GUI
    /// scale bucket changes. A theme font that cannot be built keeps the game
    /// font for that choice instead of retrying every frame.
    pub(super) fn sync_ui_font(&mut self, width: u32, height: u32) {
        let skin = self.options.menu.theme().theme().font;
        let wanted = FontChoice::for_skin(skin, gui_scale(width, height));
        if wanted == self.assets.font_choice {
            return;
        }
        let atlas = match wanted {
            FontChoice::Game => self.assets.font.atlas.clone(),
            FontChoice::Hd(cell) => theme_font::hd_atlas(cell).unwrap_or_else(|error| {
                tracing::warn!(%error, "theme font unavailable; keeping the game font");
                self.assets.font.atlas.clone()
            }),
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        match ui_font::install(renderer, &atlas) {
            Ok(installed) => {
                self.assets.ui_atlas = Some(Arc::new(installed.atlas));
                self.hud_cache.invalidate();
            }
            Err(error) => tracing::warn!(%error, "could not install the theme font"),
        }
        self.assets.font_choice = wanted;
    }
}
