// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title screen (ADR-0056).
//!
//! [`TitleScreen`] is a façade over two screens that share the same outcome
//! and the same per-frame [`TitleInfo`]:
//!
//! - [`classic`]: the vanilla screen (MCP `GuiMainMenu`);
//! - [`modern`]: Umicraft's own screen with the logo, the wordmark, Play and
//!   Options.
//!
//! The caller picks one per frame through [`TitleInfo::style`]. Both screens
//! are ticked every tick, so switching keeps the panorama angle and never
//! jumps.

mod classic;
pub mod modern;

pub use classic::{title_layout, TitleLayout, LOGO_TEXT, SPLASHES};
pub use modern::{modern_title_layout, ModernTitleLayout, WORDMARK};

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::menus::panorama::{PanoramaTextures, PanoramaView};
use crate::text::FontAtlas;

use classic::ClassicTitle;
use modern::ModernTitle;

/// Which title screen the player sees (ADR-0056).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleStyle {
    /// Umicraft's own screen: logo, wordmark, Play and Options.
    #[default]
    Modern,
    /// The vanilla 1.8 screen.
    Classic,
}

impl TitleStyle {
    /// The other style, for the toggle of the options.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Modern => Self::Classic,
            Self::Classic => Self::Modern,
        }
    }
}

/// What the player chose on the title screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleOutcome {
    /// Open the server list (Multiplayer, or Play on the modern screen).
    Multiplayer,
    /// Open the options.
    Options,
    /// Open the language choice (classic screen only).
    Language,
    /// Quit the game (classic screen only).
    Quit,
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
    /// The screen to draw.
    pub style: TitleStyle,
    /// The brand atlas (`crate::brand_font`), which measures the text of the
    /// modern screen. It must be the atlas the renderer draws
    /// [`crate::FontFace::Brand`] with.
    pub brand_font: &'a FontAtlas,
}

/// The title screen: both styles and the dispatch between them.
#[derive(Debug, Clone)]
pub struct TitleScreen {
    classic: ClassicTitle,
    modern: ModernTitle,
}

impl TitleScreen {
    /// Creates the screen; `seed` (for example the start time) picks the
    /// splash of the classic style.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            classic: ClassicTitle::new(seed),
            modern: ModernTitle::new(),
        }
    }

    /// The splash line of this start (classic style).
    #[must_use]
    pub fn splash(&self) -> &str {
        self.classic.splash()
    }

    /// Replaces the splash line (for example one read from `splashes.txt`).
    pub fn set_splash(&mut self, splash: impl Into<String>) {
        self.classic.set_splash(splash);
    }

    /// Tells the screen whether the window has the focus: the opening of the
    /// modern style waits for it.
    pub fn set_focused(&mut self, focused: bool) {
        self.modern.set_focused(focused);
    }

    /// Advances both styles by one game tick.
    pub fn tick(&mut self) {
        self.classic.tick();
        self.modern.tick();
    }

    /// Draws one frame of `info.style` and returns the entry the player
    /// chose, with the panorama the renderer must draw below the UI.
    ///
    /// `atlas` is the game font of the classic style.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        info: &TitleInfo<'_>,
    ) -> (Option<TitleOutcome>, Option<PanoramaView>) {
        match info.style {
            TitleStyle::Classic => self.classic.show(ui, atlas, lang, info),
            TitleStyle::Modern => self.modern.show(ui, lang, info),
        }
    }
}

#[cfg(test)]
mod tests;
