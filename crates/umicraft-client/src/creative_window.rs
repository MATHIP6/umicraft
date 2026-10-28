// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window-thread glue of the creative inventory: winit events become
//! [`CreativeEvent`]s for the game thread.
//!
//! This module is a child of [`crate::window`] so it can reach the window
//! state without widening its visibility. The input rules live in
//! [`crate::creative_input`], the geometry in [`crate::creative_layout`].

use std::time::Instant;

use umicraft_game::inventory::creative::MouseButton as CreativeButton;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use winit::keyboard::{KeyCode, PhysicalKey};

use super::App;
use crate::creative_input::{Context, CreativeEvent, Key, KeyKind, KeyOutcome};
use crate::creative_layout::CreativeLayout;
use crate::game::GameEvent;
use crate::inventory_screen::CREATIVE_GAMEMODE;

impl App {
    /// Whether the creative inventory is shown (a server container replaces
    /// it).
    pub(super) fn creative_screen_visible(&self) -> bool {
        self.creative_open && !self.container_is_open()
    }

    /// Opens the inventory key's screen: the creative window in creative
    /// mode, the survival inventory otherwise (MCP `Minecraft.runTick`).
    pub(super) fn open_inventory(&mut self) {
        let creative = self
            .world
            .as_ref()
            .is_some_and(|world| world.status().gamemode == CREATIVE_GAMEMODE);
        if creative {
            self.creative_open = true;
            self.creative_input.reset();
            self.send_creative(CreativeEvent::Open);
        } else {
            self.inventory_open = true;
        }
        self.capture_mouse(false);
    }

    /// The layout of the creative window in the current display.
    fn creative_layout(&self) -> Option<CreativeLayout> {
        let size = self.window.as_ref()?.inner_size();
        Some(CreativeLayout::new(size.width, size.height))
    }

    /// What the input machine needs to know, once the game thread published
    /// the first snapshot.
    fn creative_context(&self) -> Option<Context> {
        if !self.creative_screen_visible() {
            return None;
        }
        let world = self.world.as_ref()?;
        let view = world.creative_view()?;
        Some(Context {
            tab: view.tab,
            cursor_has_item: world.window_view().cursor.is_some(),
            needs_scrollbar: view.needs_scrollbar,
        })
    }

    fn send_creative(&self, event: CreativeEvent) {
        let _ = self.session.events().try_send(GameEvent::Creative(event));
    }

    /// Feeds a pointer move to the creative input.
    pub(super) fn creative_pointer(&mut self, position: PhysicalPosition<f64>) {
        let (Some(layout), Some(context)) = (self.creative_layout(), self.creative_context())
        else {
            return;
        };
        let point = layout.local((position.x as f32, position.y as f32));
        if let Some(event) = self.creative_input.pointer_moved(point, context) {
            self.send_creative(event);
        }
    }

    /// Feeds a mouse button to the creative input.
    pub(super) fn creative_button(&mut self, state: ElementState, button: MouseButton) {
        let (Some(context), Some(button)) = (self.creative_context(), creative_button(button))
        else {
            return;
        };
        let event = match state {
            ElementState::Pressed => {
                self.creative_input
                    .button_pressed(button, context, Instant::now())
            }
            ElementState::Released => self.creative_input.button_released(button, context),
        };
        if let Some(event) = event {
            self.send_creative(event);
        }
    }

    /// Feeds a mouse wheel movement to the creative input.
    pub(super) fn creative_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(context) = self.creative_context() else {
            return;
        };
        if let Some(event) = self.creative_input.wheel(wheel_steps(delta), context) {
            self.send_creative(event);
        }
    }

    /// Handles a key event while the creative window is open.
    pub(super) fn handle_creative_key(&mut self, event: &KeyEvent) {
        let (PhysicalKey::Code(code), ElementState::Pressed) = (event.physical_key, event.state)
        else {
            return;
        };
        if event.repeat && code == KeyCode::Escape {
            return;
        }
        let Some(context) = self.creative_context() else {
            return;
        };
        let text = event.text.as_ref().and_then(|text| text.chars().next());
        let key = Key {
            kind: key_kind(code),
            text: text.filter(|c| !c.is_control()),
        };
        match self.creative_input.key_pressed(key, context) {
            KeyOutcome::Close => self.close_inventory(),
            KeyOutcome::Send(event) => self.send_creative(event),
            KeyOutcome::Ignored => {}
        }
    }
}

/// The creative button of a winit mouse button, if it uses it.
fn creative_button(button: MouseButton) -> Option<CreativeButton> {
    match button {
        MouseButton::Left => Some(CreativeButton::Left),
        MouseButton::Right => Some(CreativeButton::Right),
        MouseButton::Middle => Some(CreativeButton::Middle),
        _ => None,
    }
}

/// The role of a physical key in the creative window.
fn key_kind(code: KeyCode) -> KeyKind {
    let digit = match code {
        KeyCode::Escape => return KeyKind::Escape,
        KeyCode::Backspace => return KeyKind::Backspace,
        KeyCode::KeyE => return KeyKind::Inventory,
        KeyCode::KeyT => return KeyKind::Chat,
        KeyCode::KeyQ => return KeyKind::Drop,
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        _ => return KeyKind::Other,
    };
    KeyKind::Digit(digit)
}

/// The number of wheel steps, positive when scrolling up; vanilla only looks
/// at the sign of the movement.
fn wheel_steps(delta: MouseScrollDelta) -> i32 {
    let vertical = match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(y),
        MouseScrollDelta::PixelDelta(position) => position.y,
    };
    if vertical > 0.0 {
        1
    } else if vertical < 0.0 {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_their_creative_roles() {
        assert_eq!(key_kind(KeyCode::KeyE), KeyKind::Inventory);
        assert_eq!(key_kind(KeyCode::KeyT), KeyKind::Chat);
        assert_eq!(key_kind(KeyCode::KeyQ), KeyKind::Drop);
        assert_eq!(key_kind(KeyCode::Digit9), KeyKind::Digit(9));
        assert_eq!(key_kind(KeyCode::Digit0), KeyKind::Other);
        assert_eq!(key_kind(KeyCode::KeyW), KeyKind::Other);
    }

    #[test]
    fn the_wheel_reports_only_its_direction() {
        assert_eq!(wheel_steps(MouseScrollDelta::LineDelta(0.0, 3.0)), 1);
        assert_eq!(wheel_steps(MouseScrollDelta::LineDelta(0.0, -0.5)), -1);
        assert_eq!(wheel_steps(MouseScrollDelta::LineDelta(2.0, 0.0)), 0);
        let pixels = PhysicalPosition::new(0.0, -12.0);
        assert_eq!(wheel_steps(MouseScrollDelta::PixelDelta(pixels)), -1);
    }

    #[test]
    fn only_the_three_main_buttons_are_used() {
        assert_eq!(
            creative_button(MouseButton::Middle),
            Some(CreativeButton::Middle)
        );
        assert_eq!(creative_button(MouseButton::Back), None);
    }
}
