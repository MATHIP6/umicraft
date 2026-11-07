// SPDX-License-Identifier: MIT OR Apache-2.0

//! Translation of winit buttons and key codes into the vocabulary of
//! [`crate::inventory_input`], which stays free of winit types.

use winit::event::MouseButton;

use crate::inventory_input::Button;

/// The inventory button of a winit mouse button, if it uses it.
pub fn button(button: MouseButton) -> Option<Button> {
    match button {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Right => Some(Button::Right),
        MouseButton::Middle => Some(Button::Middle),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_main_buttons_are_used() {
        assert_eq!(button(MouseButton::Middle), Some(Button::Middle));
        assert_eq!(button(MouseButton::Back), None);
    }
}
