// SPDX-License-Identifier: MIT OR Apache-2.0

//! What an [`Action`](super::Action) is bound to: a keyboard key or a mouse
//! button, as `KeyBinding` allows in the reference (a negative key code is a
//! mouse button: `-100` is "Button 1", `-99` "Button 2", `-98` "Button 3").

use umicraft_core::lang::Lang;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use super::names::{is_reserved, key_from_name, key_label, key_name};

/// The prefix of a mouse binding in `options.toml` (`Mouse1` is button 1).
const MOUSE_PREFIX: &str = "Mouse";

/// The highest mouse button number that can be bound. The reference reads
/// the buttons its LWJGL mouse reports, at most 32.
const MAX_BUTTON: u8 = 32;

/// A keyboard key or a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binding {
    /// A keyboard key.
    Key(KeyCode),
    /// A mouse button, from `0` (left, "Button 1"), `1` (right), `2`
    /// (middle) then the side buttons.
    Mouse(u8),
}

impl Binding {
    /// The binding of a winit mouse button. Left, right and middle are the
    /// first three buttons as in LWJGL; the side buttons follow.
    #[must_use]
    pub fn from_mouse(button: MouseButton) -> Option<Self> {
        let index = match button {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(code) => u8::try_from(code).ok()?.checked_add(5)?,
        };
        (index < MAX_BUTTON).then_some(Self::Mouse(index))
    }

    /// Whether the player may bind it: mouse buttons always, keys unless the
    /// client keeps them (see [`is_reserved`]) or they have no name.
    #[must_use]
    pub fn is_bindable(self) -> bool {
        match self {
            Self::Key(key) => !is_reserved(key) && key_name(key).is_some(),
            Self::Mouse(index) => index < MAX_BUTTON,
        }
    }

    /// The name `options.toml` stores: the winit key name or `MouseN`.
    #[must_use]
    pub fn name(self) -> Option<String> {
        match self {
            Self::Key(key) => key_name(key).map(str::to_string),
            Self::Mouse(index) => Some(format!("{MOUSE_PREFIX}{}", u32::from(index) + 1)),
        }
    }

    /// The binding stored under `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        if let Some(number) = name.strip_prefix(MOUSE_PREFIX) {
            let number: u8 = number.parse().ok()?;
            return number
                .checked_sub(1)
                .filter(|index| *index < MAX_BUTTON)
                .map(Self::Mouse);
        }
        key_from_name(name).map(Self::Key)
    }

    /// The text of the key button on the controls page: the key label, or
    /// `key.mouseButton` ("Button N") for a mouse button.
    #[must_use]
    pub fn label(self, lang: &Lang) -> String {
        match self {
            Self::Key(key) => key_label(key),
            Self::Mouse(index) => {
                let number = (u32::from(index) + 1).to_string();
                lang.tr_args("key.mouseButton", &[&number])
            }
        }
    }
}

impl From<KeyCode> for Binding {
    fn from(key: KeyCode) -> Self {
        Self::Key(key)
    }
}

impl PartialEq<KeyCode> for Binding {
    fn eq(&self, other: &KeyCode) -> bool {
        *self == Self::Key(*other)
    }
}
