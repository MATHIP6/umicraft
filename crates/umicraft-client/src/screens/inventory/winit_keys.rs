// SPDX-License-Identifier: MIT OR Apache-2.0

//! Translation of winit buttons and key codes into the vocabulary of
//! [`crate::screens::inventory::input`], which stays free of winit types.

use winit::event::MouseButton;

use crate::options::Binding;
use crate::screens::inventory::input::Button;

/// The inventory button of a winit mouse button, if it uses it.
///
/// `GuiContainer.mouseClicked` reacts to the left and right buttons and to
/// the one bound to pick block (`keyBindPickBlock.getKeyCode() + 100`, the
/// middle button by default). The pick-block test comes first, as there.
pub fn button(button: MouseButton, pick_block: Binding) -> Option<Button> {
    let binding = Binding::from_mouse(button)?;
    match binding {
        Binding::Mouse(index) if binding == pick_block => {
            i8::try_from(index).ok().map(Button::PickBlock)
        }
        Binding::Mouse(0) => Some(Button::Left),
        Binding::Mouse(1) => Some(Button::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use winit::keyboard::KeyCode;

    use super::*;

    const MIDDLE: Binding = Binding::Mouse(2);

    #[test]
    fn left_and_right_are_always_used() {
        assert_eq!(button(MouseButton::Left, MIDDLE), Some(Button::Left));
        assert_eq!(button(MouseButton::Right, MIDDLE), Some(Button::Right));
        assert_eq!(button(MouseButton::Back, MIDDLE), None);
    }

    #[test]
    fn the_pick_block_button_follows_its_binding() {
        assert_eq!(
            button(MouseButton::Middle, MIDDLE),
            Some(Button::PickBlock(2))
        );
        let side = Binding::Mouse(3);
        assert_eq!(button(MouseButton::Middle, side), None);
        assert_eq!(button(MouseButton::Back, side), Some(Button::PickBlock(3)));
    }

    #[test]
    fn a_keyboard_pick_block_frees_the_middle_button() {
        let key = Binding::Key(KeyCode::KeyR);
        assert_eq!(button(MouseButton::Middle, key), None);
    }
}
