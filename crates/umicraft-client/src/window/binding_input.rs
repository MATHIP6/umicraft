// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game input driven by the key bindings: a key or a mouse button, bound
//! to any action, is held or pressed through the same [`Binding`] path
//! (`Minecraft.runTick` reads `KeyBinding.isKeyDown` / `isPressed` the same
//! way for keys and mouse buttons).

use umicraft_render::RenderError;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::KeyCode;

use super::App;
use crate::options::{Action, Binding};
use crate::screenshot_file;

impl App {
    /// Whether the binding of `action` is held. Shift and Control are told
    /// apart: only the key that is bound counts, so the left Shift sneaks and
    /// the right one is free (it opens the mods menu). Alt still counts
    /// through its twin.
    pub(super) fn held(&self, action: Action) -> bool {
        match self.options.keys.get(action) {
            Binding::Key(key) => {
                self.keys.contains(&key)
                    || modifier_twin(key).is_some_and(|twin| self.keys.contains(&twin))
            }
            Binding::Mouse(index) => self.buttons.contains(&index),
        }
    }

    /// Notes that `button` went up, whatever screen is open.
    ///
    /// `Minecraft.runTick` updates the state of every key binding on each
    /// event before the screen sees it, so a button pressed in the game and
    /// released over a container the server opened (the lobby compass) is not
    /// left held for the game that comes next.
    pub(super) fn release_button(&mut self, button: MouseButton) {
        if let Some(Binding::Mouse(index)) = Binding::from_mouse(button) {
            self.buttons.remove(&index);
        }
    }

    /// Lets go of every held key and button and drops the clicks that wait
    /// (`KeyBinding.unPressAllKeys`, which `Minecraft.setIngameNotInFocus`
    /// calls when a screen takes the mouse).
    pub(super) fn unpress_all(&mut self) {
        self.keys.clear();
        self.buttons.clear();
        self.break_block = false;
        self.place_block = false;
        self.pick_block = false;
    }

    /// Runs the actions bound to a key or button that was just pressed.
    pub(super) fn press_binding(&mut self, binding: Binding) {
        let actions: Vec<Action> = self.options.keys.actions_for(binding).collect();
        for action in actions {
            // An action that opened a screen takes the press; the others wait.
            if self.screen != crate::screen::Screen::None {
                break;
            }
            self.run_action(action);
        }
    }

    fn run_action(&mut self, action: Action) {
        match action {
            Action::Chat => self.open_chat(""),
            Action::Command => self.open_chat("/"),
            Action::Inventory => self.open_inventory(),
            Action::Attack => self.break_block |= self.mouse_captured,
            Action::Use => self.place_block |= self.mouse_captured,
            Action::PickBlock => self.pick_block |= self.mouse_captured,
            Action::Drop if self.mouse_captured => self.drop_item = Some(self.drop_kind()),
            Action::TogglePerspective => self.toggle_perspective = true,
            Action::Screenshot => self.screenshot_requested = true,
            Action::Fullscreen => self.options.video.fullscreen = !self.options.video.fullscreen,
            other => {
                if let Some(slot) = other.hotbar_slot() {
                    self.select_slot = Some(slot);
                }
            }
        }
    }

    /// Writes a captured frame to the `screenshots` folder and says so in the
    /// log (the reference prints "Saved screenshot as ..." in the chat).
    pub(super) fn save_screenshot(&self, captured: Result<umicraft_render::Image, RenderError>) {
        let folder =
            umicraft_assets::cache::data_dir().map(|dir| dir.join(screenshot_file::FOLDER));
        let saved = match (captured, folder) {
            (Ok(image), Some(folder)) => {
                screenshot_file::save(&folder, &image.rgba, image.width, image.height)
                    .map_err(|error| error.to_string())
            }
            (Err(error), _) => Err(error.to_string()),
            (_, None) => Err("no data folder".to_string()),
        };
        match saved {
            Ok(path) => tracing::info!(path = %path.display(), "saved screenshot"),
            Err(error) => tracing::warn!(%error, "could not save the screenshot"),
        }
    }

    /// Handles a mouse button while the game owns the mouse. A click with the
    /// mouse free grabs it (as `Minecraft.runTick` does) and does nothing
    /// else; otherwise the button is tracked and its actions run.
    pub(super) fn handle_game_mouse(&mut self, state: ElementState, button: MouseButton) {
        let Some(Binding::Mouse(index)) = Binding::from_mouse(button) else {
            return;
        };
        match state {
            ElementState::Pressed if !self.mouse_captured => self.capture_mouse(true),
            ElementState::Pressed => {
                self.buttons.insert(index);
                self.press_binding(Binding::Mouse(index));
            }
            ElementState::Released => {
                self.buttons.remove(&index);
            }
        }
    }
}

/// The key on the other side of the keyboard that counts as the same
/// modifier: holding either Alt triggers the binding. Shift and Control have
/// no twin: the left and the right key are two different keys.
fn modifier_twin(key: KeyCode) -> Option<KeyCode> {
    match key {
        KeyCode::AltLeft => Some(KeyCode::AltRight),
        KeyCode::AltRight => Some(KeyCode::AltLeft),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_is_held_through_its_twin() {
        assert_eq!(modifier_twin(KeyCode::AltLeft), Some(KeyCode::AltRight));
        assert_eq!(modifier_twin(KeyCode::AltRight), Some(KeyCode::AltLeft));
        assert_eq!(modifier_twin(KeyCode::KeyW), None);
    }

    #[test]
    fn the_left_and_right_shift_and_control_are_different_keys() {
        for key in [
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
        ] {
            assert_eq!(modifier_twin(key), None, "{key:?}");
        }
    }
}
