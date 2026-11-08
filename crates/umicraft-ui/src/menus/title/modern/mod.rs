// SPDX-License-Identifier: MIT OR Apache-2.0

//! The modern title screen (ADR-0056), Umicraft's own design.
//!
//! Over the blurred panorama and a vignette: the Umicraft logo, the
//! "UMICRAFT" wordmark in the brand face, a **Play** button that opens the
//! server list and an **Options** button. A small rounded cross in the top
//! right corner quits the game. The version and the non-affiliation credit sit
//! in the bottom corners.
//!
//! When the game starts the screen opens in stages (see [`Stage`]): the dark
//! lifts, the logo grows into place, the letters of the wordmark come in one
//! by one, the two buttons rise in turn, then the cross and the footer fade in.
//! The opening only runs while the window has the focus, so it is not spent
//! unseen: it starts when the player first comes to the window, and waits
//! again if the focus is lost.

mod animation;
mod footer;
mod layout;
pub(crate) mod wordmark;

pub use layout::{modern_title_layout, ModernTitleLayout};
pub use wordmark::WORDMARK;

pub(crate) use animation::{lowered, scaled, Stage};
use animation::{
    BUTTON_RISE, CHROME, INTRO_TICKS, LOGO, LOGO_RISE, LOGO_START_SCALE, OPTIONS, PLAY, VEIL, WORD,
};
use footer::draw_footer;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::menus::common::Gui;
use crate::menus::panorama::{PanoramaBackground, PanoramaView};
use crate::modern::{vignette, HoverFade, ModernButton};

use super::{TitleInfo, TitleOutcome};

/// The label of the quit button: a multiplication sign reads as a close cross
/// in any face and needs no translation.
const QUIT_CROSS: &str = "\u{d7}";

/// The buttons of the screen, in [`ModernTitle::fades`] order.
const BUTTONS: [TitleOutcome; 3] = [
    TitleOutcome::Multiplayer,
    TitleOutcome::Options,
    TitleOutcome::Quit,
];

/// The modern title screen state: the panorama clock, the hover fades and the
/// opening animation.
#[derive(Debug, Clone, Default)]
pub struct ModernTitle {
    panorama: PanoramaBackground,
    fades: [HoverFade; BUTTONS.len()],
    hovered: [bool; BUTTONS.len()],
    /// Ticks the opening has run, up to [`INTRO_TICKS`]; they only count
    /// while the window has the focus.
    intro_ticks: u8,
    /// True while the window lacks the focus (not so by default).
    unfocused: bool,
}

impl ModernTitle {
    /// Creates the screen with the panorama at its start angle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Tells the screen whether the window has the focus.
    /// The opening waits for it: nobody sees an animation that plays in a
    /// window the player is not on.
    pub fn set_focused(&mut self, focused: bool) {
        self.unfocused = !focused;
    }

    /// Advances the panorama, the hover fades and the opening by one game tick.
    pub fn tick(&mut self) {
        self.panorama.tick();
        if !self.unfocused {
            self.intro_ticks = (self.intro_ticks + 1).min(INTRO_TICKS);
        }
        for (fade, hovered) in self.fades.iter_mut().zip(self.hovered) {
            fade.tick(hovered);
        }
    }

    /// Ticks since the opening began, `partial_tick` of the way into the
    /// current one. It stands still while the window lacks the focus.
    fn intro_elapsed(&self, partial_tick: f32) -> f32 {
        let running = self.intro_ticks < INTRO_TICKS && !self.unfocused;
        let partial = if running {
            partial_tick.clamp(0.0, 1.0)
        } else {
            0.0
        };
        f32::from(self.intro_ticks) + partial
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

        let elapsed = self.intro_elapsed(info.partial_tick);
        let logo = LOGO.at(elapsed);
        let (play_in, options_in, chrome) =
            (PLAY.at(elapsed), OPTIONS.at(elapsed), CHROME.at(elapsed));
        let art = ui.menu_art().brand;
        let logo_box = lowered(layout.logo, LOGO_RISE, gui.scale, logo);
        let logo_box = scaled(logo_box, LOGO_START_SCALE + (1.0 - LOGO_START_SCALE) * logo);
        wordmark::draw_logo(ui, art.logo, logo_box, logo);
        wordmark::draw_wordmark(ui, brand, layout.wordmark, gui.scale, WORD.at(elapsed));
        draw_footer(ui, brand, &gui, lang, info.version, chrome);

        ui.push_id("modern_title");
        let levels = self.fades.map(|fade| fade.level(info.partial_tick));
        let play_rect = lowered(layout.play, BUTTON_RISE, gui.scale, play_in);
        let options_rect = lowered(layout.options, BUTTON_RISE, gui.scale, options_in);
        let play = ModernButton::new(ui.id("play"), play_rect, lang.tr("umicraft.title.play"))
            .with_alpha(play_in);
        let options_id = ui.id("options");
        let options =
            ModernButton::new(options_id, options_rect, lang.tr("umicraft.title.options"))
                .with_alpha(options_in);
        let quit = ModernButton::new(ui.id("quit"), layout.quit, QUIT_CROSS).with_alpha(chrome);
        let responses = [
            play.show(ui, brand, gui.scale, levels[0]),
            options.show(ui, brand, gui.scale, levels[1]),
            quit.show(ui, brand, gui.scale, levels[2]),
        ];
        ui.pop_id();

        // The black sheet goes over everything and lifts first.
        let veil = 1.0 - VEIL.at(elapsed);
        if veil > 0.0 {
            ui.draw().rect(viewport, [0.0, 0.0, 0.0, veil]);
        }

        self.hovered = responses.map(|response| response.hovered);
        let outcome = BUTTONS
            .into_iter()
            .zip(responses)
            .find_map(|(outcome, response)| response.clicked.then_some(outcome));
        (outcome, view)
    }
}
