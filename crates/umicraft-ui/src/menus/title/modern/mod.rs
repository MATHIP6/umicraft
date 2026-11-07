// SPDX-License-Identifier: MIT OR Apache-2.0

//! The modern title screen (ADR-0056), Umicraft's own design.
//!
//! Over the blurred panorama and a vignette: the Umicraft logo, the
//! "UMICRAFT" wordmark in the brand face, a **Play** button that opens the
//! server list, an **Options** button and a small power icon to quit. The
//! version and the non-affiliation credit sit in the bottom corners.

mod layout;
mod wordmark;

pub use layout::{modern_title_layout, ModernTitleLayout};
pub use wordmark::WORDMARK;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::menus::common::Gui;
use crate::menus::panorama::{PanoramaBackground, PanoramaView};
use crate::modern::{draw_text, theme, vignette, Align, HoverFade, IconButton, ModernButton};
use crate::text::FontAtlas;

use super::{TitleInfo, TitleOutcome};

/// Distance of the footer texts from the window edges, in GUI units.
const FOOTER_MARGIN: f32 = 4.0;

/// The buttons of the screen, in [`ModernTitle::fades`] order.
const BUTTONS: [TitleOutcome; 3] = [
    TitleOutcome::Multiplayer,
    TitleOutcome::Options,
    TitleOutcome::Quit,
];

/// The modern title screen state: the panorama clock and the hover fades.
#[derive(Debug, Clone, Default)]
pub struct ModernTitle {
    panorama: PanoramaBackground,
    fades: [HoverFade; BUTTONS.len()],
    hovered: [bool; BUTTONS.len()],
}

impl ModernTitle {
    /// Creates the screen with the panorama at its start angle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the panorama and the hover fades by one game tick.
    pub fn tick(&mut self) {
        self.panorama.tick();
        for (fade, hovered) in self.fades.iter_mut().zip(self.hovered) {
            fade.tick(hovered);
        }
    }

    /// The hover level button `index` reaches at the end of this tick.
    #[cfg(test)]
    pub(super) fn fade_level(&self, index: usize) -> f32 {
        self.fades[index].level(1.0)
    }

    /// Draws one frame and returns the entry the player chose, with the
    /// panorama the renderer must draw below the UI.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        lang: &Lang,
        info: &TitleInfo<'_>,
    ) -> (Option<TitleOutcome>, Option<PanoramaView>) {
        let view = self.panorama.draw(ui, info.panorama, info.partial_tick);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = modern_title_layout(viewport);
        let brand = info.brand_font;
        vignette(ui, viewport);
        let art = ui.menu_art().brand;
        wordmark::draw_logo(ui, art.logo, layout.logo);
        wordmark::draw_wordmark(ui, brand, layout.wordmark, gui.scale);
        Self::draw_footer(ui, brand, &gui, lang, info.version);

        ui.push_id("modern_title");
        let levels = self.fades.map(|fade| fade.level(info.partial_tick));
        let play = ModernButton::new(ui.id("play"), layout.play, lang.tr("umicraft.title.play"));
        let options_id = ui.id("options");
        let options = ModernButton::new(
            options_id,
            layout.options,
            lang.tr("umicraft.title.options"),
        );
        let responses = [
            play.show(ui, brand, gui.scale, levels[0]),
            options.show(ui, brand, gui.scale, levels[1]),
            IconButton::new(ui.id("quit"), layout.quit, art.power)
                .round()
                .show(ui, gui.scale, levels[2]),
        ];
        ui.pop_id();

        self.hovered = responses.map(|response| response.hovered);
        let outcome = BUTTONS
            .into_iter()
            .zip(responses)
            .find_map(|(outcome, response)| response.clicked.then_some(outcome));
        (outcome, view)
    }

    /// The version at the bottom left and the credit at the bottom right.
    fn draw_footer(ui: &mut Ui, brand: &FontAtlas, gui: &Gui, lang: &Lang, version: &str) {
        let size = theme::TEXT_SIZE_SMALL * gui.scale;
        let y = gui
            .rect(
                0.0,
                gui.height - FOOTER_MARGIN - theme::TEXT_SIZE_SMALL * 0.5,
                0.0,
                0.0,
            )
            .y;
        let left = gui.rect(FOOTER_MARGIN, 0.0, 0.0, 0.0).x;
        let right = gui.rect(gui.width - FOOTER_MARGIN, 0.0, 0.0, 0.0).x;
        draw_text(
            ui,
            brand,
            version,
            (left, y),
            (size, theme::TEXT_MUTED, Align::Left),
        );
        let credit = lang.tr("umicraft.title.credit");
        draw_text(
            ui,
            brand,
            credit,
            (right, y),
            (size, theme::TEXT_MUTED, Align::Right),
        );
    }
}
