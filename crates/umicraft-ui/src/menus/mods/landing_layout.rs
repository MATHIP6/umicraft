// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the parts of the landing page of the mods menu sit: the logo, the wordmark, the Mods
//! button and the hint in a group centred on the screen, and the close cross in
//! the top right corner.

use crate::geom::Rect;
use crate::menus::common::Gui;
use crate::modern::theme;

/// Edge of the square logo box, in GUI units.
const LOGO_SIZE: f32 = 30.0;
/// Gap between the logo and the wordmark.
const LOGO_GAP: f32 = 2.0;
/// Height of the wordmark band.
const WORDMARK_HEIGHT: f32 = 12.0;
/// The wordmark is drawn at this share of its title-screen size.
pub(super) const WORDMARK_SCALE: f32 = 0.55;
/// Gap between the wordmark and the button, and between the button and the hint.
const SECTION_GAP: f32 = 8.0;
/// Width of the Mods button.
const BUTTON_WIDTH: f32 = 112.0;
/// Height of the Mods button.
const BUTTON_HEIGHT: f32 = 20.0;
/// Gap between the hint and the line explaining how to move the widgets.
const TIP_GAP: f32 = 3.0;
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
    /// Where the hint is centred.
    pub hint: [f32; 2],
    /// Where the line about moving the HUD widgets is centred, under the hint.
    /// It sits outside the centred group, so the group is as it always was.
    pub tip: [f32; 2],
    /// The close cross in the top right corner of the screen.
    pub close: Rect,
}

/// Computes the layout for `viewport`: the group of logo, wordmark, button and
/// hint is centred on the screen.
#[must_use]
pub fn landing_layout(viewport: Rect) -> LandingLayout {
    let gui = Gui::new(viewport);
    let cx = gui.width * 0.5;
    let block = LOGO_SIZE
        + LOGO_GAP
        + WORDMARK_HEIGHT
        + SECTION_GAP
        + BUTTON_HEIGHT
        + SECTION_GAP
        + theme::TEXT_SIZE_SMALL;
    let top = ((gui.height - block) * 0.5).floor();
    let wordmark = top + LOGO_SIZE + LOGO_GAP;
    let button = wordmark + WORDMARK_HEIGHT + SECTION_GAP;
    let hint_y = button + BUTTON_HEIGHT + SECTION_GAP + theme::TEXT_SIZE_SMALL * 0.5;
    let hint = gui.rect(cx, hint_y, 0.0, 0.0);
    let tip = gui.rect(cx, hint_y + theme::TEXT_SIZE_SMALL + TIP_GAP, 0.0, 0.0);
    LandingLayout {
        logo: gui.rect(cx - LOGO_SIZE * 0.5, top, LOGO_SIZE, LOGO_SIZE),
        wordmark: gui.rect(0.0, wordmark, gui.width, WORDMARK_HEIGHT),
        mods: gui.rect(cx - BUTTON_WIDTH * 0.5, button, BUTTON_WIDTH, BUTTON_HEIGHT),
        hint: [hint.x, hint.y],
        tip: [tip.x, tip.y],
        close: gui.rect(
            gui.width - CLOSE_MARGIN - CLOSE_SIZE,
            CLOSE_MARGIN,
            CLOSE_SIZE,
            CLOSE_SIZE,
        ),
    }
}
