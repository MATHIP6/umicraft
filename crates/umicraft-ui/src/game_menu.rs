// SPDX-License-Identifier: MIT OR Apache-2.0

//! The in-game menu opened with Escape (M10, ADR-0017).
//!
//! [`GameMenu`] is a **pure** screen in the same style as
//! [`crate::debug_screen::DebugScreen`]: it draws through a [`Ui`] and a
//! [`FontAtlas`], reads no file, clock or network, and reports what the player
//! chose as a value. The client decides what "resume" and "quit" mean. M10
//! only needs these two entries; the options screen is added by the settings
//! work and the full pause screen belongs to M12.

use crate::context::Ui;
use crate::draw::TextStyle;
use crate::geom::{Layout, Rect};
use crate::text::FontAtlas;
use crate::widgets::{button, panel, style, PANEL_INSET};

/// Width of one menu button, in pixels.
const BUTTON_WIDTH: f32 = 240.0;
/// Height of one menu button, in pixels.
const BUTTON_HEIGHT: f32 = 32.0;
/// Gap between two stacked rows, in pixels.
const SPACING: f32 = 8.0;
/// Scale of the menu title.
const TITLE_SCALE: f32 = 3.0;
/// Title of the menu.
const TITLE: &str = "GAME MENU";
/// Label of the entry that closes the menu.
const RESUME_LABEL: &str = "BACK TO GAME";
/// Label of the entry that leaves the client.
const QUIT_LABEL: &str = "QUIT";

/// What the player chose in the menu during one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMenuAction {
    /// Close the menu and return to the game.
    Resume,
    /// Disconnect and leave the client.
    Quit,
}

/// The layout of the game menu from the last frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct GameMenu {
    resume_rect: Rect,
    quit_rect: Rect,
}

impl GameMenu {
    /// Creates the menu; the rectangles are [`Rect::ZERO`] until the first
    /// [`Self::show`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Draws one frame of the menu and returns the entry the player clicked.
    ///
    /// `ui` must be between [`Ui::begin_frame`] and [`Ui::end_frame`].
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas) -> Option<GameMenuAction> {
        let viewport = ui.viewport();
        ui.draw().rect(viewport, style::SCREEN_BACKDROP);

        let title_height = atlas.line_height(TITLE_SCALE);
        let height = title_height + 2.0 * BUTTON_HEIGHT + 2.0 * SPACING + 2.0 * PANEL_INSET;
        let width = BUTTON_WIDTH + 2.0 * PANEL_INSET;
        let outer = Rect::new(
            viewport.x + (viewport.width - width) * 0.5,
            viewport.y + (viewport.height - height) * 0.5,
            width,
            height,
        );
        let content = panel(ui, outer, style::PANEL);
        let mut layout = Layout::vertical(content).with_spacing(SPACING);
        let title_rect = layout.next_fixed(title_height);
        let resume_rect = layout.next(content.width, BUTTON_HEIGHT);
        let quit_rect = layout.next(content.width, BUTTON_HEIGHT);
        self.resume_rect = resume_rect;
        self.quit_rect = quit_rect;

        ui.push_id("game-menu");
        let resume_id = ui.id("resume");
        let quit_id = ui.id("quit");
        ui.draw().text(
            [title_rect.x, title_rect.y],
            TITLE,
            TextStyle::new(TITLE_SCALE, style::TITLE),
        );
        let resume = button(ui, resume_id, resume_rect, atlas, RESUME_LABEL).clicked;
        let quit = button(ui, quit_id, quit_rect, atlas, QUIT_LABEL).clicked;
        ui.pop_id();

        if resume {
            Some(GameMenuAction::Resume)
        } else if quit {
            Some(GameMenuAction::Quit)
        } else {
            None
        }
    }

    /// The rectangle of the resume button in the last frame.
    #[must_use]
    pub fn resume_rect(&self) -> Rect {
        self.resume_rect
    }

    /// The rectangle of the quit button in the last frame.
    #[must_use]
    pub fn quit_rect(&self) -> Rect {
        self.quit_rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, PointerButton, UiInput};

    fn frame(menu: &mut GameMenu, input: UiInput) -> Option<GameMenuAction> {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, input);
        let action = menu.show(&mut ui, &FontAtlas::fallback());
        ui.end_frame();
        action
    }

    fn click_at(rect: Rect) -> UiInput {
        let position = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        UiInput::new(vec![
            InputEvent::PointerMoved { position },
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            },
        ])
    }

    #[test]
    fn an_idle_frame_chooses_nothing_and_lays_out_both_buttons() {
        let mut menu = GameMenu::new();
        assert_eq!(frame(&mut menu, UiInput::default()), None);
        assert!(!menu.resume_rect().is_empty());
        assert!(menu.quit_rect().y > menu.resume_rect().y);
    }

    #[test]
    fn clicking_resume_and_quit_report_their_action() {
        let mut menu = GameMenu::new();
        frame(&mut menu, UiInput::default());
        let resume_input = click_at(menu.resume_rect());
        let resume = frame(&mut menu, resume_input);
        assert_eq!(resume, Some(GameMenuAction::Resume));
        let quit_input = click_at(menu.quit_rect());
        let quit = frame(&mut menu, quit_input);
        assert_eq!(quit, Some(GameMenuAction::Quit));
    }

    #[test]
    fn the_menu_is_centred_in_the_viewport() {
        let mut menu = GameMenu::new();
        frame(&mut menu, UiInput::default());
        let centre = menu.resume_rect().x + menu.resume_rect().width * 0.5;
        assert!((centre - 400.0).abs() < 1.0);
    }
}
