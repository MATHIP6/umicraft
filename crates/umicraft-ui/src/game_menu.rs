// SPDX-License-Identifier: MIT OR Apache-2.0

//! The in-game pause menu opened with Escape (spec §12.2, ADR-0017).
//!
//! Replicates `net.minecraft.client.gui.GuiIngameMenu` from Minecraft 1.8.8
//! at authentic GUI scale (`gui_scale`).
//!
//! Coordinates in GUI space (200x20 and 98x20 buttons, `cx = width / 2`, `h4 = height / 4`):
//! - Title "Game Menu" centered at `(cx, 40)`
//! - Button 4 (`Back to Game`): `(cx - 100, h4 + 8)`, 200x20
//! - Button 5 (`Achievements`): `(cx - 100, h4 + 32)`, 98x20
//! - Button 6 (`Statistics`): `(cx + 2, h4 + 32)`, 98x20
//! - Button 0 (`Options...`): `(cx - 100, h4 + 80)`, 98x20
//! - Button 7 (`Open to LAN`): `(cx + 2, h4 + 80)`, 98x20 (disabled in multiplayer)
//! - Button 1 (`Disconnect` / `Save and Quit to Title`): `(cx - 100, h4 + 104)`, 200x20

use crate::context::Ui;
use crate::draw::TextStyle;
use crate::geom::Rect;
use crate::text::FontAtlas;
use crate::widgets::{button, button_enabled, gui_scale};

/// The localized labels displayed by the game menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMenuLabels {
    /// Screen title (vanilla `menu.game`, default "Game Menu").
    pub title: String,
    /// Return to game (vanilla `menu.returnToGame`, default "Back to Game").
    pub resume: String,
    /// Achievements (vanilla `gui.achievements`, default "Achievements").
    pub achievements: String,
    /// Statistics (vanilla `gui.stats`, default "Statistics").
    pub stats: String,
    /// Options (vanilla `menu.options`, default "Options...").
    pub options: String,
    /// Open to LAN (vanilla `menu.shareToLan`, default "Open to LAN").
    pub share_to_lan: String,
    /// Disconnect (vanilla `menu.disconnect`, default "Disconnect").
    pub disconnect: String,
    /// Return to menu in singleplayer (vanilla `menu.returnToMenu`, default "Save and Quit to Title").
    pub return_to_menu: String,
}

impl Default for GameMenuLabels {
    fn default() -> Self {
        Self {
            title: "Game Menu".to_string(),
            resume: "Back to Game".to_string(),
            achievements: "Achievements".to_string(),
            stats: "Statistics".to_string(),
            options: "Options...".to_string(),
            share_to_lan: "Open to LAN".to_string(),
            disconnect: "Disconnect".to_string(),
            return_to_menu: "Save and Quit to Title".to_string(),
        }
    }
}

/// What the player chose in the menu during one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMenuAction {
    /// Close the menu and return to the game (vanilla button 4).
    Resume,
    /// Open achievements screen (vanilla button 5).
    Achievements,
    /// Open statistics screen (vanilla button 6).
    Stats,
    /// Open options screen (vanilla button 0).
    Options,
    /// Disconnect and leave the client (vanilla button 1).
    Quit,
}

/// The layout of the game menu from the last frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct GameMenu {
    resume_rect: Rect,
    achievements_rect: Rect,
    stats_rect: Rect,
    options_rect: Rect,
    lan_rect: Rect,
    quit_rect: Rect,
    is_singleplayer: bool,
}

impl GameMenu {
    /// Creates the menu; the rectangles are [`Rect::ZERO`] until the first
    /// [`Self::show`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether the client is in singleplayer mode.
    ///
    /// In singleplayer, button 1 is labeled "Save and Quit to Title" instead of
    /// "Disconnect", and "Open to LAN" is enabled.
    pub fn set_singleplayer(&mut self, singleplayer: bool) {
        self.is_singleplayer = singleplayer;
    }

    /// Returns whether the menu is configured for singleplayer.
    #[must_use]
    pub fn is_singleplayer(&self) -> bool {
        self.is_singleplayer
    }

    /// Draws one frame of the menu using default vanilla labels and returns
    /// the entry the player clicked.
    ///
    /// `ui` must be between [`Ui::begin_frame`] and [`Ui::end_frame`].
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas) -> Option<GameMenuAction> {
        let labels = GameMenuLabels::default();
        self.show_with_labels(ui, atlas, &labels)
    }

    /// Draws one frame of the menu with customized/localized labels and
    /// returns the entry the player clicked.
    pub fn show_with_labels(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        labels: &GameMenuLabels,
    ) -> Option<GameMenuAction> {
        let viewport = ui.viewport();

        // 1. Draw default gradient background: -1072689136 (0xC0101010) to -804253680 (0xD0101010)
        draw_default_background(ui);

        // 2. Compute vanilla ScaledResolution GUI scale
        let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
        let scaled_w = (viewport.width / scale).ceil();
        let scaled_h = (viewport.height / scale).ceil();
        let cx = (scaled_w * 0.5).floor();
        let h4 = (scaled_h * 0.25).floor();
        let i = -16.0;

        // 3. Draw centered title at y = 40 in GUI units with shadow
        let title_w = atlas.measure(&labels.title, scale);
        let title_x = (cx * scale - title_w * 0.5).round();
        let title_y = (40.0 * scale).round();
        ui.draw().text(
            [title_x + scale, title_y + scale],
            &labels.title,
            TextStyle::new(scale, [0.25, 0.25, 0.25, 1.0]),
        );
        ui.draw().text(
            [title_x, title_y],
            &labels.title,
            TextStyle::new(scale, [1.0, 1.0, 1.0, 1.0]),
        );

        // 4. Button rectangles in physical pixels
        let resume_rect = Rect::new(
            (cx - 100.0) * scale,
            (h4 + 24.0 + i) * scale,
            200.0 * scale,
            20.0 * scale,
        );
        let achievements_rect = Rect::new(
            (cx - 100.0) * scale,
            (h4 + 48.0 + i) * scale,
            98.0 * scale,
            20.0 * scale,
        );
        let stats_rect = Rect::new(
            (cx + 2.0) * scale,
            (h4 + 48.0 + i) * scale,
            98.0 * scale,
            20.0 * scale,
        );
        let options_rect = Rect::new(
            (cx - 100.0) * scale,
            (h4 + 96.0 + i) * scale,
            98.0 * scale,
            20.0 * scale,
        );
        let lan_rect = Rect::new(
            (cx + 2.0) * scale,
            (h4 + 96.0 + i) * scale,
            98.0 * scale,
            20.0 * scale,
        );
        let quit_rect = Rect::new(
            (cx - 100.0) * scale,
            (h4 + 120.0 + i) * scale,
            200.0 * scale,
            20.0 * scale,
        );

        self.resume_rect = resume_rect;
        self.achievements_rect = achievements_rect;
        self.stats_rect = stats_rect;
        self.options_rect = options_rect;
        self.lan_rect = lan_rect;
        self.quit_rect = quit_rect;

        ui.push_id("game-menu");
        let resume_id = ui.id("resume");
        let achievements_id = ui.id("achievements");
        let stats_id = ui.id("stats");
        let options_id = ui.id("options");
        let lan_id = ui.id("lan");
        let quit_id = ui.id("quit");

        let quit_label = if self.is_singleplayer {
            &labels.return_to_menu
        } else {
            &labels.disconnect
        };

        let resume = button(ui, resume_id, resume_rect, atlas, &labels.resume).clicked;
        let achievements = button(
            ui,
            achievements_id,
            achievements_rect,
            atlas,
            &labels.achievements,
        )
        .clicked;
        let stats = button(ui, stats_id, stats_rect, atlas, &labels.stats).clicked;
        let options = button(ui, options_id, options_rect, atlas, &labels.options).clicked;
        let _lan = button_enabled(
            ui,
            lan_id,
            lan_rect,
            atlas,
            &labels.share_to_lan,
            self.is_singleplayer,
        );
        let quit = button(ui, quit_id, quit_rect, atlas, quit_label).clicked;
        ui.pop_id();

        if resume {
            Some(GameMenuAction::Resume)
        } else if achievements {
            Some(GameMenuAction::Achievements)
        } else if stats {
            Some(GameMenuAction::Stats)
        } else if options {
            Some(GameMenuAction::Options)
        } else if quit {
            Some(GameMenuAction::Quit)
        } else {
            None
        }
    }

    /// The rectangle of the resume button ("Back to Game") in the last frame.
    #[must_use]
    pub fn resume_rect(&self) -> Rect {
        self.resume_rect
    }

    /// The rectangle of the achievements button in the last frame.
    #[must_use]
    pub fn achievements_rect(&self) -> Rect {
        self.achievements_rect
    }

    /// The rectangle of the statistics button in the last frame.
    #[must_use]
    pub fn stats_rect(&self) -> Rect {
        self.stats_rect
    }

    /// The rectangle of the options button in the last frame.
    #[must_use]
    pub fn options_rect(&self) -> Rect {
        self.options_rect
    }

    /// The rectangle of the LAN button in the last frame.
    #[must_use]
    pub fn lan_rect(&self) -> Rect {
        self.lan_rect
    }

    /// The rectangle of the quit / disconnect button in the last frame.
    #[must_use]
    pub fn quit_rect(&self) -> Rect {
        self.quit_rect
    }
}

/// Draws the default in-game gradient backdrop (`GuiScreen.drawDefaultBackground`).
///
/// In Minecraft 1.8, in-game screens draw a vertical gradient from `-1072689136`
/// (`0xC0101010`, alpha ≈ 0.753) at the top to `-804253680` (`0xD0101010`, alpha ≈ 0.816)
/// at the bottom.
pub fn draw_default_background(ui: &mut Ui) {
    let viewport = ui.viewport();
    let steps = 16;
    let step_h = viewport.height / steps as f32;
    for idx in 0..steps {
        let frac = (idx as f32 + 0.5) / steps as f32;
        let alpha = 0.753 + (0.816 - 0.753) * frac;
        let y = viewport.y + idx as f32 * step_h;
        let h = if idx == steps - 1 {
            (viewport.y + viewport.height - y).max(0.0)
        } else {
            step_h
        };
        ui.draw().rect(
            Rect::new(viewport.x, y, viewport.width, h),
            [0.0627, 0.0627, 0.0627, alpha],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, PointerButton, UiInput};

    fn frame(menu: &mut GameMenu, input: UiInput) -> Option<GameMenuAction> {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
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
    fn an_idle_frame_chooses_nothing_and_lays_out_vanilla_buttons() {
        let mut menu = GameMenu::new();
        assert_eq!(frame(&mut menu, UiInput::default()), None);

        // Check button presence and vertical ordering:
        // Back to game (h4 + 8) < Achievements/Stats (h4 + 32) < Options/LAN (h4 + 80) < Quit (h4 + 104)
        assert!(!menu.resume_rect().is_empty());
        assert_eq!(menu.achievements_rect().y, menu.stats_rect().y);
        assert!(menu.achievements_rect().y > menu.resume_rect().y);
        assert_eq!(menu.options_rect().y, menu.lan_rect().y);
        assert!(menu.options_rect().y > menu.achievements_rect().y);
        assert!(menu.quit_rect().y > menu.options_rect().y);

        // Widths: 200 scaled px for wide buttons, 98 scaled px for split buttons
        let s = gui_scale(1280, 720) as f32; // 3.0
        assert_eq!(menu.resume_rect().width, 200.0 * s);
        assert_eq!(menu.quit_rect().width, 200.0 * s);
        assert_eq!(menu.achievements_rect().width, 98.0 * s);
        assert_eq!(menu.stats_rect().width, 98.0 * s);
        assert_eq!(menu.options_rect().width, 98.0 * s);
        assert_eq!(menu.lan_rect().width, 98.0 * s);

        // Heights: all 20 scaled px
        assert_eq!(menu.resume_rect().height, 20.0 * s);
        assert_eq!(menu.quit_rect().height, 20.0 * s);
        assert_eq!(menu.options_rect().height, 20.0 * s);
    }

    #[test]
    fn clicking_each_button_reports_action() {
        let mut menu = GameMenu::new();
        frame(&mut menu, UiInput::default());

        let resume_rect = menu.resume_rect();
        assert_eq!(
            frame(&mut menu, click_at(resume_rect)),
            Some(GameMenuAction::Resume)
        );
        let achievements_rect = menu.achievements_rect();
        assert_eq!(
            frame(&mut menu, click_at(achievements_rect)),
            Some(GameMenuAction::Achievements)
        );
        let stats_rect = menu.stats_rect();
        assert_eq!(
            frame(&mut menu, click_at(stats_rect)),
            Some(GameMenuAction::Stats)
        );
        let options_rect = menu.options_rect();
        assert_eq!(
            frame(&mut menu, click_at(options_rect)),
            Some(GameMenuAction::Options)
        );
        let quit_rect = menu.quit_rect();
        assert_eq!(
            frame(&mut menu, click_at(quit_rect)),
            Some(GameMenuAction::Quit)
        );
    }

    #[test]
    fn lan_button_is_disabled_in_multiplayer() {
        let mut menu = GameMenu::new();
        menu.set_singleplayer(false);
        frame(&mut menu, UiInput::default());
        let lan_rect = menu.lan_rect();
        let action = frame(&mut menu, click_at(lan_rect));
        // Disabled button does not trigger action
        assert_eq!(action, None);
    }

    #[test]
    fn exact_vanilla_coordinates_at_scale_3() {
        let mut menu = GameMenu::new();
        frame(&mut menu, UiInput::default());

        // Viewport 1280x720 -> scale = 3 -> scaled_w = 427, scaled_h = 240
        // cx = floor(427 * 0.5) = 213, h4 = floor(240 * 0.25) = 60
        // resume: (213 - 100) * 3 = 113 * 3 = 339; (60 + 8) * 3 = 68 * 3 = 204
        assert_eq!(menu.resume_rect().x, 339.0);
        assert_eq!(menu.resume_rect().y, 204.0);

        // achievements: (213 - 100) * 3 = 339; (60 + 32) * 3 = 92 * 3 = 276
        assert_eq!(menu.achievements_rect().x, 339.0);
        assert_eq!(menu.achievements_rect().y, 276.0);

        // stats: (213 + 2) * 3 = 215 * 3 = 645; y = 276
        assert_eq!(menu.stats_rect().x, 645.0);
        assert_eq!(menu.stats_rect().y, 276.0);

        // options: 339; (60 + 80) * 3 = 140 * 3 = 420
        assert_eq!(menu.options_rect().x, 339.0);
        assert_eq!(menu.options_rect().y, 420.0);

        // lan: 645; y = 420
        assert_eq!(menu.lan_rect().x, 645.0);
        assert_eq!(menu.lan_rect().y, 420.0);

        // quit: 339; (60 + 104) * 3 = 164 * 3 = 492
        assert_eq!(menu.quit_rect().x, 339.0);
        assert_eq!(menu.quit_rect().y, 492.0);
    }
}
