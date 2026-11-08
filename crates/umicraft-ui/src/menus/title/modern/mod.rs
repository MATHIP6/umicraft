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

mod layout;
pub(crate) mod wordmark;

pub use layout::{modern_title_layout, ModernTitleLayout};
pub use wordmark::WORDMARK;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::Gui;
use crate::menus::panorama::{PanoramaBackground, PanoramaView};
use crate::modern::theme::with_opacity;
use crate::modern::{draw_text, theme, vignette, Align, HoverFade, ModernButton};
use crate::text::FontAtlas;

use super::{TitleInfo, TitleOutcome};

/// Distance of the footer texts from the window edges, in GUI units.
const FOOTER_MARGIN: f32 = 4.0;

/// The label of the quit button: a multiplication sign reads as a close cross
/// in any face and needs no translation.
const QUIT_CROSS: &str = "\u{d7}";

/// The buttons of the screen, in [`ModernTitle::fades`] order.
const BUTTONS: [TitleOutcome; 3] = [
    TitleOutcome::Multiplayer,
    TitleOutcome::Options,
    TitleOutcome::Quit,
];

/// Ticks the opening lasts (2.6 s at 20 TPS): the end of the last stage.
const INTRO_TICKS: u8 = 52;
/// How far the logo rises while it grows into place, in GUI units.
const LOGO_RISE: f32 = 16.0;
/// Size of the logo when it starts to grow, as a fraction of its final size.
const LOGO_START_SCALE: f32 = 0.8;
/// How far a main button rises while it fades in, in GUI units.
const BUTTON_RISE: f32 = 8.0;

/// One stage of the opening: it starts `delay` ticks in and takes `span`
/// ticks, easing out.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stage {
    delay: f32,
    span: f32,
}

/// The black sheet over the screen lifts.
const VEIL: Stage = Stage::new(0.0, 24.0);
/// The logo grows and rises into place.
const LOGO: Stage = Stage::new(6.0, 26.0);
/// The letters of the wordmark come in.
const WORD: Stage = Stage::new(14.0, 24.0);
/// The Play button rises.
const PLAY: Stage = Stage::new(28.0, 18.0);
/// The Options button rises, a little after Play.
const OPTIONS: Stage = Stage::new(33.0, 18.0);
/// The quit cross and the footer fade in last.
const CHROME: Stage = Stage::new(40.0, 12.0);

impl Stage {
    pub(crate) const fn new(delay: f32, span: f32) -> Self {
        Self { delay, span }
    }

    /// `0.0` before the stage, `1.0` after it, in between a cubic ease-out of
    /// the way through it, `ticks` being the time since the opening began.
    pub(crate) fn at(self, ticks: f32) -> f32 {
        let linear = ((ticks - self.delay) / self.span).clamp(0.0, 1.0);
        1.0 - (1.0 - linear).powi(3)
    }
}

/// `rect` moved down by what is left of `rise` GUI units, `progress` of the
/// way through its stage.
pub(crate) fn lowered(rect: Rect, rise: f32, scale: f32, progress: f32) -> Rect {
    Rect::new(
        rect.x,
        rect.y + (1.0 - progress) * rise * scale,
        rect.width,
        rect.height,
    )
}

/// `rect` shrunk by `factor` about its centre.
pub(crate) fn scaled(rect: Rect, factor: f32) -> Rect {
    let (width, height) = (rect.width * factor, rect.height * factor);
    Rect::new(
        rect.x + (rect.width - width) * 0.5,
        rect.y + (rect.height - height) * 0.5,
        width,
        height,
    )
}

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
        Self::draw_footer(ui, brand, &gui, lang, info.version, chrome);

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

    /// The version at the bottom left and the credit at the bottom right,
    /// `alpha` opaque.
    fn draw_footer(
        ui: &mut Ui,
        brand: &FontAtlas,
        gui: &Gui,
        lang: &Lang,
        version: &str,
        alpha: f32,
    ) {
        let size = theme::TEXT_SIZE_SMALL * gui.scale;
        let color = with_opacity(theme::TEXT_MUTED, alpha);
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
        draw_text(ui, brand, version, (left, y), (size, color, Align::Left));
        let credit = lang.tr("umicraft.title.credit");
        draw_text(ui, brand, credit, (right, y), (size, color, Align::Right));
    }
}
