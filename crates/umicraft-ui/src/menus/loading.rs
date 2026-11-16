// SPDX-License-Identifier: MIT OR Apache-2.0

//! The loading / splash screen shown while heavy assets load in the background.

use umicraft_core::lang::Lang;

use super::background::draw_menu_background;
use crate::context::Ui;
use crate::text::FontAtlas;

use super::common::{Gui, TextPlacement, DIM_GRAY, LINE_HEIGHT};

/// The loading splash screen.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadingScreen;

impl LoadingScreen {
    /// Creates the screen.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Draws one frame.
    pub fn show(&self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) {
        draw_menu_background(ui);
        let gui = Gui::new(ui.viewport());
        let y = (gui.height / 2.0).floor() - 30.0;
        gui.title(ui, atlas, "UMICRAFT", gui.cx(), y);
        let msg = lang.get("menu.loading").unwrap_or("Loading resources...");
        let at = [gui.cx(), y + LINE_HEIGHT + 12.0];
        gui.text(ui, atlas, msg, TextPlacement::centered(at, DIM_GRAY));
    }
}
