// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the parts of the landing page of the mods menu sit: the logo, the
//! wordmark and the Mods button in a group centred on the screen, and the close
//! cross in the top right corner.

use crate::geom::Rect;

use super::layout::menu_gui;

/// Edge of the square logo box, in GUI units.
const LOGO_SIZE: f32 = 36.0;
/// Gap between the logo and the wordmark.
const LOGO_GAP: f32 = 8.0;
/// Height of the wordmark band.
const WORDMARK_HEIGHT: f32 = 14.0;
/// The wordmark is drawn at this share of its title-screen size.
pub(super) const WORDMARK_SCALE: f32 = 0.6;
/// Gap between the wordmark and the button.
const SECTION_GAP: f32 = 17.0;
/// Width of the Mods button.
const BUTTON_WIDTH: f32 = 110.0;
/// Height of the Mods button.
const BUTTON_HEIGHT: f32 = 25.0;
/// Edge of the square close cross.
const CLOSE_SIZE: f32 = 14.0;
/// Distance of the close cross from the top and right edges.
const CLOSE_MARGIN: f32 = 7.0;

/// The rectangles of the landing page, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingLayout {
    /// The square box the logo is fitted into.
    pub logo: Rect,
    /// The band the wordmark is centred in.
    pub wordmark: Rect,
    /// The Mods button.
    pub mods: Rect,
    /// The close cross in the top right corner of the screen.
    pub close: Rect,
}

/// Computes the layout for `viewport`: the group of logo, wordmark and button
/// is centred on the screen.
#[must_use]
pub fn landing_layout(viewport: Rect) -> LandingLayout {
    let gui = menu_gui(viewport);
    let cx = gui.width * 0.5;
    let block = LOGO_SIZE + LOGO_GAP + WORDMARK_HEIGHT + SECTION_GAP + BUTTON_HEIGHT;
    let top = ((gui.height - block) * 0.5).floor();
    let wordmark = top + LOGO_SIZE + LOGO_GAP;
    let button = wordmark + WORDMARK_HEIGHT + SECTION_GAP;
    LandingLayout {
        logo: gui.rect(cx - LOGO_SIZE * 0.5, top, LOGO_SIZE, LOGO_SIZE),
        wordmark: gui.rect(0.0, wordmark, gui.width, WORDMARK_HEIGHT),
        mods: gui.rect(cx - BUTTON_WIDTH * 0.5, button, BUTTON_WIDTH, BUTTON_HEIGHT),
        close: gui.rect(
            gui.width - CLOSE_MARGIN - CLOSE_SIZE,
            CLOSE_MARGIN,
            CLOSE_SIZE,
            CLOSE_SIZE,
        ),
    }
}
