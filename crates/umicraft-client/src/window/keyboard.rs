// SPDX-License-Identifier: MIT OR Apache-2.0

//! The keys: bindings to actions, the menu keys and the debug toggles.

use std::time::Instant;

use umicraft_game::item_use::DropKind;
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::App;
use crate::options::{Action, Binding};
use crate::screen::Screen;
use crate::ui_input::shortcut_code;

/// True when this key event must toggle the debug screen.
///
/// Only a fresh F4 press toggles: releases and auto-repeats are ignored so a
/// held key cannot flicker the screen.
pub(super) fn is_debug_toggle(code: KeyCode, state: ElementState, repeat: bool) -> bool {
    code == KeyCode::F4 && state == ElementState::Pressed && !repeat
}

impl App {
    /// The key of `event` as a text field reads it: with Control held, the
    /// shortcut letters follow the layout (Control+A on an AZERTY keyboard is
    /// not the key at the QWERTY `A`). Gameplay keys keep their position.
    pub(super) fn text_key_code(&self, event: &KeyEvent) -> Option<KeyCode> {
        let PhysicalKey::Code(code) = event.physical_key else {
            return None;
        };
        Some(if self.ctrl_held {
            shortcut_code(event, code)
        } else {
            code
        })
    }

    pub(super) fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        // A key that goes up is released for the game whatever screen is open
        // (`Minecraft.runTick` sets the key binding state before the screen).
        if event.state == ElementState::Released {
            self.keys.remove(&code);
        }
        // F4 is the Umicraft debug-screen toggle (F3 is the vanilla text
        // screen, F2 stays free for the screenshot). It is handled before the
        // UI translation so it can never be typed into the screen, and it is
        // ignored while the key repeats so one press toggles exactly once.
        // Releases are swallowed as well, so no F4 event ever reaches the UI.
        if code == KeyCode::F4 {
            // The debug screen shows a game; menus have none.
            if self.session.is_some() && is_debug_toggle(code, event.state, event.repeat) {
                self.set_screen(self.screen().after_debug_toggle());
            }
            return;
        }
        // The screenshot key works on every screen, the options aside (a key
        // being rebound there must reach the page): in the reference tick it
        // sits outside the key handling of the screens.
        if event.state == ElementState::Pressed
            && !event.repeat
            && self.screen() != Screen::Options
            && matches!(self.options.keys.get(Action::Screenshot), Binding::Key(key) if key == code)
        {
            self.screenshot_requested = true;
            return;
        }
        if self.take_mods_shortcut(&event, code) {
            return;
        }
        match self.screen() {
            Screen::Debug | Screen::PackPrompt => {
                self.ui_input.key(code, event.state, event.text.as_deref());
                return;
            }
            Screen::Chat => {
                self.handle_chat_key(event);
                return;
            }
            _ if self.death_screen_visible() => {
                // MCP GuiGameOver.keyTyped does nothing: Escape does not close it.
                return;
            }
            Screen::Creative => {
                self.handle_creative_key(&event);
                return;
            }
            Screen::Inventory => {
                self.handle_inventory_key(event_loop, event);
                return;
            }
            Screen::Sign | Screen::Book => {
                self.handle_text_screen_key(&event);
                return;
            }
            Screen::GameMenu => {
                self.handle_menu_key(event);
                return;
            }
            Screen::Options => {
                self.handle_options_key(event);
                return;
            }
            Screen::Mods => {
                self.handle_mods_key(&event);
                return;
            }
            Screen::Title
            | Screen::Loading
            | Screen::ServerList
            | Screen::ServerEdit
            | Screen::ConfirmDelete
            | Screen::Accounts
            | Screen::Connecting
            | Screen::Disconnected
            | Screen::Stats
            | Screen::Achievements => {
                self.handle_menu_screen_key(&event);
                return;
            }
            Screen::None => {}
        }
        match event.state {
            ElementState::Pressed => {
                if !event.repeat {
                    match code {
                        KeyCode::Escape => {
                            self.set_screen(Screen::None.after_escape(self.options_origin))
                        }
                        KeyCode::F1 => self.show_overlay = !self.show_overlay,
                        KeyCode::F3 => {
                            self.show_f3 = !self.show_f3;
                            if self.show_f3 {
                                umicraft_core::profiling::set_alloc_tracking(true);
                                self.last_alloc_snapshot =
                                    umicraft_core::profiling::AllocSnapshot::now();
                            } else if !umicraft_core::profiling::is_enabled() {
                                umicraft_core::profiling::set_alloc_tracking(false);
                            }
                        }
                        // F3 + P toggles the runtime profiler and allocation tracker.
                        KeyCode::KeyP if self.keys.contains(&KeyCode::F3) => {
                            let enabled = umicraft_core::profiling::toggle();
                            if !enabled && self.show_f3 {
                                umicraft_core::profiling::set_alloc_tracking(true);
                            }
                        }
                        // F3 + T reloads the resource packs, whatever the
                        // chat key is bound to.
                        KeyCode::KeyT if self.keys.contains(&KeyCode::F3) => {
                            self.reload_resources();
                        }
                        _ => self.press_binding(Binding::Key(code)),
                    }
                }
                self.keys.insert(code);
                self.last_input_time = Instant::now();
                self.publish_input();
            }
            ElementState::Released => {
                self.keys.remove(&code);
                self.last_input_time = Instant::now();
                self.publish_input();
            }
        }
    }

    /// What the drop key drops: the whole stack with Ctrl held
    /// (MCP Minecraft.runTick line 2084, GuiScreen.isCtrlKeyDown).
    pub(super) fn drop_kind(&self) -> DropKind {
        if self.keys.contains(&KeyCode::ControlLeft) || self.keys.contains(&KeyCode::ControlRight) {
            DropKind::Stack
        } else {
            DropKind::One
        }
    }

    /// Handles a key press while the game menu is open.
    fn handle_menu_key(&mut self, event: KeyEvent) {
        let Some(code) = self.text_key_code(&event) else {
            return;
        };
        if code == KeyCode::Escape {
            if event.state == ElementState::Pressed && !event.repeat {
                self.set_screen(Screen::GameMenu.after_escape(self.options_origin));
            }
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
    }
}
