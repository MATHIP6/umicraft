// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the mods menu: the right Shift key that opens and closes it,
//! the keys it takes, and the blur it puts over the game.
//!
//! The menu itself is drawn by `MenuFlow` (see `menu_flow/mods.rs`) and laid
//! out in `umicraft_ui::menus::mods`; the blur is applied by the renderer.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use super::App;
use crate::menu_flow::MenuFlow;
use crate::screen::Screen;

/// How blurred the game behind the current screen is, `0.0` to `1.0`: only the
/// mods menu blurs it, and the blur fades in while the menu opens.
pub(super) fn background_blur(screen: Screen, menu: &MenuFlow) -> f32 {
    if screen == Screen::Mods {
        menu.mods_blur()
    } else {
        0.0
    }
}

impl App {
    /// The right Shift key opens the mods menu over a game and closes it
    /// again; the left one stays the sneak key. It is taken before the key
    /// bindings so it triggers no action bound to it, and only where it means
    /// something: elsewhere it stays a plain modifier. Returns whether the key
    /// was taken.
    pub(super) fn take_mods_shortcut(&mut self, event: &KeyEvent, code: KeyCode) -> bool {
        let applies = code == KeyCode::ShiftRight
            && self.session.is_some()
            && !self.death_screen_visible()
            && matches!(self.screen(), Screen::None | Screen::Mods);
        if applies && event.state == ElementState::Pressed && !event.repeat {
            self.toggle_mods_menu();
        }
        applies
    }

    /// Opens the mods menu over the game, or closes it when it is open.
    fn toggle_mods_menu(&mut self) {
        let next = if self.screen == Screen::Mods {
            Screen::None
        } else {
            Screen::Mods
        };
        self.set_screen(next);
    }

    /// Hands a key to the mods menu: Escape closes it, the rest goes to the UI.
    pub(super) fn handle_mods_key(&mut self, event: &KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if code == KeyCode::Escape {
            if event.state == ElementState::Pressed && !event.repeat {
                self.set_screen(Screen::None);
            }
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
    }
}
