// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title screen (MCP `GuiMainMenu`).
//!
//! Layout in GUI units, `cx = width / 2`, `j = height / 4 + 48`:
//! - Singleplayer `(cx - 100, j)` 200x20 (disabled: Umicraft only plays on
//!   servers, so the button is shown greyed as a vanilla user expects it)
//! - Multiplayer `(cx - 100, j + 24)` 200x20
//! - Options `(cx - 100, j + 84)` 98x20, Quit Game `(cx + 2, j + 84)` 98x20
//! - Language `(cx - 124, j + 84)` 20x20 (`GuiButtonLanguage`)
//!
//!
//! The Minecraft Realms row (`j + 48`) is not drawn, by the owner's decision.
//! The logo is the
//! vanilla texture from the user's client jar when [`crate::menus::MenuArt`]
//! has it (owner-approved, never committed, spec §3) and the original word
//! "Umicraft" otherwise. The splash is tilted by -20 degrees and pulses like
//! vanilla; it comes from the user's `splashes.txt` when the caller sets one
//! ([`TitleScreen::set_splash`]) and from an original list otherwise.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::{Color, SpriteSheet, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;
use crate::widgets::style;

use super::common::{Gui, TextPlacement, WHITE};
use super::panorama::{PanoramaBackground, PanoramaTextures, PanoramaView};

/// The product name drawn as the logo.
pub const LOGO_TEXT: &str = "Umicraft";
/// Original splash lines; one is picked per start (`GuiMainMenu` reads
/// `splashes.txt`, which is a Mojang asset and not used here).
pub const SPLASHES: [&str; 8] = [
    "Multiplayer only!",
    "Written in Rust!",
    "Made of crates!",
    "No unwraps here!",
    "Immediate mode!",
    "Hello, server!",
    "Tick tock!",
    "Open source!",
];
/// The yellow of the splash (`0xFFFF00`).
const SPLASH_COLOR: Color = [1.0, 1.0, 0.0, 1.0];
/// The turn of the splash (`rotate(-20, 0, 0, 1)`).
const SPLASH_TURN_DEGREES: f32 = -20.0;
/// The warm colour of the logo.
const LOGO_COLOR: Color = [0.96, 0.78, 0.28, 1.0];
/// The logo text is this many times the GUI scale (the vanilla logo is 44
/// units high, 32 units fit its band).
const LOGO_FACTOR: f32 = 4.0;
/// Width of one half of the logo texture, in texels and GUI units.
const LOGO_HALF_WIDTH: f32 = 155.0;
/// Height of the logo, in texels and GUI units.
const LOGO_HEIGHT: f32 = 44.0;
/// The row of the second half in `minecraft.png`.
const LOGO_HALF_ROW: u32 = 45;
/// The top of the logo in GUI units.
const LOGO_TOP: f32 = 30.0;

/// What the player chose on the title screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleOutcome {
    /// Open the server list.
    Multiplayer,
    /// Open the options.
    Options,
    /// Open the language choice.
    Language,
    /// Quit the game.
    Quit,
}

/// The rectangles of the title screen buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitleLayout {
    /// The disabled Singleplayer button.
    pub singleplayer: Rect,
    /// The Multiplayer button.
    pub multiplayer: Rect,
    /// The Options button.
    pub options: Rect,
    /// The Quit Game button.
    pub quit: Rect,
    /// The Language button.
    pub language: Rect,
}

/// Computes the vanilla layout of the title screen for `viewport`.
#[must_use]
pub fn title_layout(viewport: Rect) -> TitleLayout {
    let gui = Gui::new(viewport);
    let (cx, j) = (gui.cx(), gui.h_div(4.0) + 48.0);
    TitleLayout {
        singleplayer: gui.rect(cx - 100.0, j, 200.0, 20.0),
        multiplayer: gui.rect(cx - 100.0, j + 24.0, 200.0, 20.0),
        options: gui.rect(cx - 100.0, j + 84.0, 98.0, 20.0),
        quit: gui.rect(cx + 2.0, j + 84.0, 98.0, 20.0),
        language: gui.rect(cx - 124.0, j + 84.0, 20.0, 20.0),
    }
}

/// What the caller supplies each frame.
#[derive(Debug, Clone, Copy)]
pub struct TitleInfo<'a> {
    /// The version line at the bottom left (for example `Umicraft 0.1.0`).
    pub version: &'a str,
    /// The uploaded panorama faces, if the textures are installed.
    pub panorama: Option<PanoramaTextures>,
    /// The fraction of a tick since the last [`TitleScreen::tick`].
    pub partial_tick: f32,
}

/// The title screen state: the splash and the panorama clock.
#[derive(Debug, Clone)]
pub struct TitleScreen {
    splash: String,
    panorama: PanoramaBackground,
}

impl TitleScreen {
    /// Creates the screen; `seed` (for example the start time) picks the splash.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            splash: SPLASHES[(seed % SPLASHES.len() as u64) as usize].to_string(),
            panorama: PanoramaBackground::new(),
        }
    }

    /// The splash line of this start.
    #[must_use]
    pub fn splash(&self) -> &str {
        &self.splash
    }

    /// Replaces the splash line (for example one read from `splashes.txt`).
    pub fn set_splash(&mut self, splash: impl Into<String>) {
        self.splash = splash.into();
    }

    /// Advances the panorama and the splash pulse by one game tick.
    pub fn tick(&mut self) {
        self.panorama.tick();
    }

    /// Draws one frame and returns the entry the player chose.
    ///
    /// The second value is the panorama the renderer must draw below the UI
    /// (see [`PanoramaBackground::draw`]).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        info: &TitleInfo<'_>,
    ) -> (Option<TitleOutcome>, Option<PanoramaView>) {
        let view = self.panorama.draw(ui, info.panorama, info.partial_tick);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = title_layout(viewport);
        self.draw_logo(ui, atlas, &gui, info.partial_tick);
        gui.text(
            ui,
            atlas,
            info.version,
            TextPlacement::left([2.0, gui.height - 10.0], WHITE),
        );
        let credit = lang.tr("umicraft.title.credit");
        let credit_x = gui.width - atlas.measure(credit, gui.scale) / gui.scale - 2.0;
        gui.text(
            ui,
            atlas,
            credit,
            TextPlacement::left([credit_x, gui.height - 10.0], WHITE),
        );

        ui.push_id("title");
        let multiplayer_id = ui.id("multiplayer");
        let options_id = ui.id("options");
        let quit_id = ui.id("quit");
        let singleplayer_id = ui.id("singleplayer");
        let singleplayer = lang.tr("menu.singleplayer");
        gui.button(
            ui,
            singleplayer_id,
            layout.singleplayer,
            atlas,
            singleplayer,
            false,
        );
        let multiplayer = gui.button(
            ui,
            multiplayer_id,
            layout.multiplayer,
            atlas,
            lang.tr("menu.multiplayer"),
            true,
        );
        let options = gui.button(
            ui,
            options_id,
            layout.options,
            atlas,
            lang.tr("menu.options"),
            true,
        );
        let quit = gui.button(ui, quit_id, layout.quit, atlas, lang.tr("menu.quit"), true);
        let language = self.language_button(ui, layout.language);
        ui.pop_id();

        let outcome = if multiplayer {
            Some(TitleOutcome::Multiplayer)
        } else if options {
            Some(TitleOutcome::Options)
        } else if language {
            Some(TitleOutcome::Language)
        } else if quit {
            Some(TitleOutcome::Quit)
        } else {
            None
        };
        (outcome, view)
    }

    /// The vanilla logo: two 155x44 blits of `minecraft.png`, the left half
    /// at `(0, 0)` and the right half at `(0, 45)`, side by side from
    /// `x = width / 2 - 137`, `y = 30` (`GuiMainMenu.drawScreen`). The rare
    /// easter-egg variant that reorders the left half is not reproduced.
    fn draw_logo_texture(ui: &mut Ui, gui: &Gui, sheet: SpriteSheet) {
        let left = gui.cx() - 137.0;
        let halves = [(left, 0), (left + LOGO_HALF_WIDTH, LOGO_HALF_ROW)];
        for (x, v) in halves {
            let src = sheet.sub(0, v, LOGO_HALF_WIDTH as u32, LOGO_HEIGHT as u32);
            let dst = gui.rect(x, LOGO_TOP, LOGO_HALF_WIDTH, LOGO_HEIGHT);
            ui.draw().image(src, dst, WHITE);
        }
    }

    /// The logo band and the pulsing splash.
    fn draw_logo(&self, ui: &mut Ui, atlas: &FontAtlas, gui: &Gui, partial_tick: f32) {
        let cx = gui.cx();
        match ui.menu_art().logo {
            Some(sheet) => Self::draw_logo_texture(ui, gui, sheet),
            None => {
                let logo = TextPlacement {
                    at: [cx, 30.0 + 6.0],
                    color: LOGO_COLOR,
                    factor: LOGO_FACTOR,
                    centered: true,
                };
                gui.text(ui, atlas, LOGO_TEXT, logo);
            }
        }

        // Vanilla pulses with the wall clock (`getSystemTime() % 1000`); the
        // tick count keeps the screen free of a clock, 20 ticks = 1 second.
        self.draw_splash(ui, atlas, gui, partial_tick);
    }

    /// The splash: translate to `(width / 2 + 90, 70)`, turn by -20 degrees,
    /// scale by `(1.8 - |sin(t) * 0.1|) * 100 / (width + 32)` and draw the
    /// text centred at `y = -8` (`GuiMainMenu.drawScreen`).
    fn draw_splash(&self, ui: &mut Ui, atlas: &FontAtlas, gui: &Gui, partial_tick: f32) {
        // Vanilla pulses with the wall clock (`getSystemTime() % 1000`); the
        // tick count keeps the screen free of a clock, 20 ticks = 1 second.
        let phase = ((self.panorama.ticks() % 20) as f32 + partial_tick) / 20.0;
        let pulse = 1.8 - ((phase * std::f32::consts::TAU).sin() * 0.1).abs();
        let units = atlas.measure(&self.splash, 1.0);
        let scale = gui.scale * pulse * 100.0 / (units + 32.0);
        let angle = SPLASH_TURN_DEGREES.to_radians();
        let pivot = gui.rect(gui.cx() + 90.0, 70.0, 0.0, 0.0);
        let (sin, cos) = angle.sin_cos();
        // The text origin in turned space, then turned around the pivot.
        let place = |local: [f32; 2]| {
            [
                pivot.x + local[0] * cos - local[1] * sin,
                pivot.y + local[0] * sin + local[1] * cos,
            ]
        };
        let local = [-units * scale / 2.0, -8.0 * scale];
        let style = |color: Color| TextStyle::new(scale, color).rotated(angle);
        let shadow = [
            SPLASH_COLOR[0] * 0.25,
            SPLASH_COLOR[1] * 0.25,
            SPLASH_COLOR[2] * 0.25,
            1.0,
        ];
        let shadow_at = place([local[0] + scale, local[1] + scale]);
        ui.draw()
            .text(shadow_at, self.splash.as_str(), style(shadow));
        ui.draw()
            .text(place(local), self.splash.as_str(), style(SPLASH_COLOR));
    }

    /// The 20x20 language button with the globe cell of `widgets.png`.
    fn language_button(&self, ui: &mut Ui, rect: Rect) -> bool {
        let id = ui.id("language");
        let response = ui.response(id, rect);
        match ui.widgets_sheet() {
            Some(sheet) => {
                let v = if response.hovered { 126 } else { 106 };
                ui.draw().image(sheet.sub(0, v, 20, 20), rect, [1.0; 4]);
            }
            None => {
                let fill = if response.hovered {
                    style::BUTTON_HOVER
                } else {
                    style::BUTTON
                };
                ui.draw().rect(rect, fill);
            }
        }
        response.clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::UiInput;
    use crate::menus::common::testing::{click_at, frame, VIEWPORT};

    fn show(
        screen: &mut TitleScreen,
        input: UiInput,
        panorama: Option<PanoramaTextures>,
    ) -> (Option<TitleOutcome>, Option<PanoramaView>) {
        let atlas = FontAtlas::fallback();
        let lang = Lang::builtin();
        let info = TitleInfo {
            version: "Umicraft test",
            panorama,
            partial_tick: 0.0,
        };
        frame(input, |ui| screen.show(ui, &atlas, &lang, &info))
    }

    #[test]
    fn the_layout_has_the_vanilla_coordinates() {
        let layout = title_layout(VIEWPORT);
        // scale 3, cx = 213, j = 60 + 48 = 108
        assert_eq!(layout.singleplayer, Rect::new(339.0, 324.0, 600.0, 60.0));
        assert_eq!(layout.multiplayer.y, 396.0);
        assert_eq!(layout.options, Rect::new(339.0, 576.0, 294.0, 60.0));
        assert_eq!(layout.quit.x, 645.0);
        assert_eq!(layout.language, Rect::new(267.0, 576.0, 60.0, 60.0));
    }

    #[test]
    fn each_button_reports_its_outcome() {
        let layout = title_layout(VIEWPORT);
        let mut screen = TitleScreen::new(0);
        for (rect, expected) in [
            (layout.multiplayer, TitleOutcome::Multiplayer),
            (layout.options, TitleOutcome::Options),
            (layout.language, TitleOutcome::Language),
            (layout.quit, TitleOutcome::Quit),
        ] {
            assert_eq!(show(&mut screen, click_at(rect), None).0, Some(expected));
        }
    }

    #[test]
    fn singleplayer_is_disabled() {
        let layout = title_layout(VIEWPORT);
        let mut screen = TitleScreen::new(0);
        assert_eq!(
            show(&mut screen, click_at(layout.singleplayer), None).0,
            None
        );
    }

    #[test]
    fn the_seed_picks_a_splash_from_the_list() {
        assert_eq!(TitleScreen::new(3).splash(), SPLASHES[3]);
        assert_eq!(TitleScreen::new(8).splash(), SPLASHES[0]);
    }

    #[test]
    fn the_panorama_is_returned_only_with_textures() {
        let mut screen = TitleScreen::new(0);
        assert_eq!(show(&mut screen, UiInput::default(), None).1, None);
        let textures = PanoramaTextures { faces: [1; 6] };
        screen.tick();
        let view = show(&mut screen, UiInput::default(), Some(textures)).1;
        assert_eq!(view.map(|v| v.textures), Some(textures));
    }

    #[test]
    fn the_logo_texture_is_two_155_by_44_blits_at_the_vanilla_position() {
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let mut screen = TitleScreen::new(0);
        let commands = frame(UiInput::default(), |ui| {
            ui.set_menu_art(crate::menus::MenuArt {
                dirt: None,
                logo: Some(sheet),
                ..crate::menus::MenuArt::default()
            });
            let atlas = FontAtlas::fallback();
            let info = TitleInfo {
                version: "v",
                panorama: None,
                partial_tick: 0.0,
            };
            screen.show(ui, &atlas, &Lang::builtin(), &info);
            ui.draw().commands().to_vec()
        });
        // Scale 3, cx = 213: x = (213 - 137) * 3, y = 30 * 3.
        let logo: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                crate::draw::DrawCommand::Image { src, dst, .. } if dst.height == 132.0 => {
                    Some((*src, *dst))
                }
                _ => None,
            })
            .collect();
        assert_eq!(logo.len(), 2);
        assert_eq!(logo[0].1, Rect::new(228.0, 90.0, 465.0, 132.0));
        assert_eq!(logo[1].1, Rect::new(693.0, 90.0, 465.0, 132.0));
        assert_eq!(logo[0].0, sheet.sub(0, 0, 155, 44));
        assert_eq!(logo[1].0, sheet.sub(0, 45, 155, 44));
    }

    #[test]
    fn without_the_texture_the_text_logo_is_drawn() {
        let mut screen = TitleScreen::new(0);
        let drawn = frame(UiInput::default(), |ui| {
            let atlas = FontAtlas::fallback();
            let info = TitleInfo {
                version: "v",
                panorama: None,
                partial_tick: 0.0,
            };
            screen.show(ui, &atlas, &Lang::builtin(), &info);
            ui.draw().commands().iter().any(|command| {
                matches!(command, crate::draw::DrawCommand::Text { text, .. } if text == LOGO_TEXT)
            })
        });
        assert!(drawn);
    }
}
