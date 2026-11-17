// SPDX-License-Identifier: MIT OR Apache-2.0

//! The death screen: when it shows and the respawn button.

use std::time::Instant;

use umicraft_ui::widgets::BUTTON_DELAY_TICKS;

use super::App;
use crate::game::GameEvent;
use crate::screen::Screen;

impl App {
    /// True while the local player is dead (MCP `getHealth() <= 0`).
    fn player_dead(&self) -> bool {
        self.world
            .as_ref()
            .is_some_and(|world| world.hud().health <= 0.0)
    }

    /// True while the death screen is shown: the player is dead and has not
    /// asked to respawn yet.
    pub(super) fn death_screen_visible(&self) -> bool {
        self.death_started.is_some() && !self.respawn_requested
    }

    /// Opens or closes the death screen as the health changes
    /// (MCP `Minecraft.runTick`: a dead player gets a `GuiGameOver`).
    pub(super) fn update_death_screen(&mut self, now: Instant) {
        if !self.player_dead() {
            self.death_started = None;
            self.respawn_requested = false;
            return;
        }
        if self.death_started.is_none() {
            self.death_started = Some(now);
            self.set_screen(Screen::None);
            self.capture_mouse(false);
        }
    }

    /// Whether the death-screen buttons accept clicks yet.
    pub(super) fn death_buttons_enabled(&self, now: Instant) -> bool {
        self.death_started.is_some_and(|started| {
            now.duration_since(started) >= crate::game::TICK * BUTTON_DELAY_TICKS
        })
    }

    /// Asks the server to respawn and gives the game back to the player.
    pub(super) fn respawn(&mut self) {
        // MCP `GuiGameOver.actionPerformed` (button 0): `respawnPlayer`, then
        // the screen closes at once.
        self.send_event(GameEvent::Respawn);
        self.respawn_requested = true;
        self.ui_input.clear();
        self.capture_mouse(true);
    }
}
