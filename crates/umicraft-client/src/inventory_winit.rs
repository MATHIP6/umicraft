// SPDX-License-Identifier: MIT OR Apache-2.0

//! Translation of winit buttons and key codes into the vocabulary of
//! [`crate::inventory_input`], which stays free of winit types.

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use crate::inventory_input::{Button, InventoryKey};

/// The inventory button of a winit mouse button, if it uses it.
pub fn button(button: MouseButton) -> Option<Button> {
    match button {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Right => Some(Button::Right),
        MouseButton::Middle => Some(Button::Middle),
        _ => None,
    }
}

/// The inventory key of a physical key code, if it uses it.
pub fn key(code: KeyCode) -> Option<InventoryKey> {
    let digit = match code {
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        KeyCode::KeyQ => return Some(InventoryKey::Drop),
        _ => return None,
    };
    Some(InventoryKey::Number(digit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_and_q_map_to_inventory_keys() {
        assert_eq!(key(KeyCode::Digit4), Some(InventoryKey::Number(4)));
        assert_eq!(key(KeyCode::KeyQ), Some(InventoryKey::Drop));
        assert_eq!(key(KeyCode::KeyW), None);
    }

    #[test]
    fn only_the_three_main_buttons_are_used() {
        assert_eq!(button(MouseButton::Middle), Some(Button::Middle));
        assert_eq!(button(MouseButton::Back), None);
    }
}
