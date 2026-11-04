// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the menu screens (ADR-0044): the keys they take, the
//! commands `MenuFlow` returns and the pause menu's entries.
//!
//! The screens are drawn by `build_ui_frame`, which owns the borrows of the
//! UI frame; what the player chose is applied here, after the frame.

use std::time::Duration;

use umicraft_ui::game_menu::GameMenuAction;
use winit::event::KeyEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;

use super::App;
use crate::game::GameEvent;
use crate::menu_flow::MenuCommand;
use crate::options_screen::Page;
use crate::screen::Screen;

/// The version line of the title screen.
pub(super) const VERSION_LINE: &str = concat!("Umicraft ", env!("CARGO_PKG_VERSION"));

impl App {
    /// Hands a key to the UI of a menu screen; the screens read Escape and
    /// Enter themselves.
    pub(super) fn handle_menu_screen_key(&mut self, event: &KeyEvent) {
        if let PhysicalKey::Code(code) = event.physical_key {
            self.ui_input.key(code, event.state, event.text.as_deref());
        }
    }

    /// Advances the title animation and the double-click timer.
    pub(super) fn advance_menus(&mut self, elapsed: Duration) {
        if self.screen.uses_ui() {
            self.menu.advance(elapsed);
        }
    }

    /// Applies the command a menu screen produced in the last UI frame.
    pub(super) fn apply_menu_command(
        &mut self,
        command: MenuCommand,
        event_loop: &ActiveEventLoop,
    ) {
        match command {
            MenuCommand::Connect(address) => {
                self.remember_last_server();
                self.connect(&address);
            }
            MenuCommand::OpenOptions { page } => self.open_options(page),
            MenuCommand::Quit => {
                self.save_options();
                event_loop.exit();
            }
            // Leaving the connecting screen for the list cancels the attempt.
            MenuCommand::GoTo(Screen::ServerList) if self.screen == Screen::Connecting => {
                self.disconnect();
            }
            MenuCommand::GoTo(screen) => self.set_screen(screen),
        }
    }

    /// Applies the entry the player clicked in the pause menu.
    pub(super) fn apply_game_menu_action(&mut self, action: GameMenuAction) {
        match action {
            GameMenuAction::Resume => {
                self.set_screen(Screen::None);
                self.capture_mouse(true);
            }
            GameMenuAction::Options => self.open_options(Page::Main),
            // Out of P10: vanilla opens GuiAchievements here (docs/parity.md).
            GameMenuAction::Achievements => {}
            GameMenuAction::Stats => {
                self.send_event(GameEvent::RequestStats);
                self.set_screen(Screen::Stats);
            }
            GameMenuAction::Quit => self.disconnect(),
        }
    }

    /// Saves the address of a direct connection as the option that fills the
    /// direct-connect field next time.
    fn remember_last_server(&mut self) {
        let last = self.menu.last_server();
        if self.options.multiplayer.last_server != last {
            self.options.multiplayer.last_server = last.to_string();
            self.save_options();
        }
    }
}
