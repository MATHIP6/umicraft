// SPDX-License-Identifier: MIT OR Apache-2.0

//! The layout of the modern title screen, in GUI units times the GUI scale.
//!
//! Original design (ADR-0056): the logo and the wordmark sit just above the
//! vertical centre and the two buttons just below it. Everything is centred
//! on `width / 2`, so the screen keeps its balance from the smallest window to
//! a wide monitor. The quit cross sits alone in the top right corner.

use crate::geom::Rect;
use crate::menus::common::Gui;

/// Edge of the square logo box.
const LOGO_SIZE: f32 = 56.0;
/// Gap between the bottom of the logo and the wordmark.
const LOGO_GAP: f32 = 6.0;
/// Height of the wordmark band.
pub(super) const WORDMARK_HEIGHT: f32 = 22.0;
/// Gap between the wordmark and the first button.
const WORDMARK_GAP: f32 = 18.0;
/// Size of one main button.
const BUTTON_WIDTH: f32 = 160.0;
/// Height of one main button.
const BUTTON_HEIGHT: f32 = 22.0;
/// Vertical gap between the main buttons.
const BUTTON_GAP: f32 = 6.0;
/// Edge of the square quit button: a quarter of the logo cube, so the two
/// keep their proportion at every GUI scale.
const QUIT_SIZE: f32 = LOGO_SIZE * 0.25;
/// Distance of the quit button from the top and right edges: half its edge.
const QUIT_MARGIN: f32 = QUIT_SIZE * 0.5;

/// The rectangles of the modern title screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModernTitleLayout {
    /// The square box the logo is fitted into.
    pub logo: Rect,
    /// The band the "UMICRAFT" wordmark is centred in.
    pub wordmark: Rect,
    /// The Play button (opens the server list).
    pub play: Rect,
    /// The Options button.
    pub options: Rect,
    /// The cross in the top right corner that quits the game.
    pub quit: Rect,
}

/// Computes the modern layout for `viewport`.
///
/// The group of logo, wordmark and buttons is centred vertically as one
/// block, so no part leaves the window at the smallest GUI height (240).
#[must_use]
pub fn modern_title_layout(viewport: Rect) -> ModernTitleLayout {
    let gui = Gui::new(viewport);
    let cx = gui.width * 0.5;
    let block =
        LOGO_SIZE + LOGO_GAP + WORDMARK_HEIGHT + WORDMARK_GAP + 2.0 * BUTTON_HEIGHT + BUTTON_GAP;
    let top = ((gui.height - block) * 0.5).floor();
    let wordmark_top = top + LOGO_SIZE + LOGO_GAP;
    let play_top = wordmark_top + WORDMARK_HEIGHT + WORDMARK_GAP;
    let button = |y: f32| gui.rect(cx - BUTTON_WIDTH * 0.5, y, BUTTON_WIDTH, BUTTON_HEIGHT);
    ModernTitleLayout {
        logo: gui.rect(cx - LOGO_SIZE * 0.5, top, LOGO_SIZE, LOGO_SIZE),
        wordmark: gui.rect(0.0, wordmark_top, gui.width, WORDMARK_HEIGHT),
        play: button(play_top),
        options: button(play_top + BUTTON_HEIGHT + BUTTON_GAP),
        quit: gui.rect(
            gui.width - QUIT_MARGIN - QUIT_SIZE,
            QUIT_MARGIN,
            QUIT_SIZE,
            QUIT_SIZE,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::common::testing::VIEWPORT;

    #[test]
    fn the_block_is_centred_horizontally() {
        let layout = modern_title_layout(VIEWPORT);
        let centre = VIEWPORT.x + VIEWPORT.width * 0.5;
        // The GUI width rounds up (`ScaledResolution`), so the centre may
        // move by up to half a GUI unit.
        let half_unit = Gui::new(VIEWPORT).scale * 0.5;
        for rect in [layout.logo, layout.play, layout.options] {
            assert!(
                (rect.x + rect.width * 0.5 - centre).abs() <= half_unit,
                "{rect:?}"
            );
        }
    }

    #[test]
    fn the_parts_stack_top_to_bottom_without_overlap() {
        let layout = modern_title_layout(VIEWPORT);
        let bottom = |rect: Rect| rect.y + rect.height;
        assert!(bottom(layout.logo) <= layout.wordmark.y);
        assert!(bottom(layout.wordmark) <= layout.play.y);
        assert!(bottom(layout.play) <= layout.options.y);
        assert!(bottom(layout.options) <= VIEWPORT.y + VIEWPORT.height);
    }

    #[test]
    fn the_quit_cross_sits_in_the_top_right_corner_clear_of_the_block() {
        let layout = modern_title_layout(VIEWPORT);
        let right = VIEWPORT.x + VIEWPORT.width;
        assert!(layout.quit.x + layout.quit.width < right && layout.quit.y > VIEWPORT.y);
        assert!(
            (layout.quit.width - layout.quit.height).abs() < 1e-3,
            "square"
        );
        assert!(layout.quit.x >= layout.play.x + layout.play.width);
    }

    #[test]
    fn the_quit_cross_is_a_quarter_of_the_logo_cube() {
        let layout = modern_title_layout(VIEWPORT);
        assert!((layout.quit.width * 4.0 - layout.logo.width).abs() < 1e-2);
    }

    #[test]
    fn the_smallest_window_keeps_everything_visible() {
        let viewport = Rect::new(0.0, 0.0, 320.0, 240.0);
        let layout = modern_title_layout(viewport);
        assert!(layout.logo.y >= 0.0);
        assert!(layout.options.y + layout.options.height <= 240.0);
    }
}
