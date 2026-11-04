// SPDX-License-Identifier: MIT OR Apache-2.0

//! The keys a player can bind: their `options.toml` names and their labels.

use winit::keyboard::KeyCode;

/// Lists `(name, key)` pairs for the keys a player can bind; the name is the
/// winit variant name, which is what `options.toml` stores.
macro_rules! bindable_keys {
    ($($key:ident),* $(,)?) => {
        &[$((stringify!($key), KeyCode::$key)),*]
    };
}

pub(super) const BINDABLE: &[(&str, KeyCode)] = bindable_keys![
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Space,
    Tab,
    Enter,
    Backspace,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    Slash,
    Backslash,
    Comma,
    Period,
    Semicolon,
    Quote,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backquote,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    CapsLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
];

/// The name `options.toml` stores for a key, or `None` for a key that cannot
/// be bound.
#[must_use]
pub fn key_name(key: KeyCode) -> Option<&'static str> {
    BINDABLE
        .iter()
        .find(|(_, candidate)| *candidate == key)
        .map(|(name, _)| *name)
}

/// The key stored under `name`.
#[must_use]
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    BINDABLE
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, key)| *key)
}

/// Whether the client keeps a key for itself: `Escape` closes screens and
/// `F1` to `F4` toggle the overlay, the debug screen, the `F3` combinations
/// and the resource reload.
#[must_use]
pub fn is_reserved(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Escape | KeyCode::F1 | KeyCode::F2 | KeyCode::F3 | KeyCode::F4
    )
}

/// The text shown for a key on the controls screen.
#[must_use]
pub fn key_label(key: KeyCode) -> String {
    let Some(name) = key_name(key) else {
        return "?".to_string();
    };
    if let Some(letter) = name.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        return digit.to_string();
    }
    if let Some(digit) = name.strip_prefix("Numpad") {
        return format!("NUM {digit}");
    }
    let label = match name {
        "ShiftLeft" => "LEFT SHIFT",
        "ShiftRight" => "RIGHT SHIFT",
        "ControlLeft" => "LEFT CTRL",
        "ControlRight" => "RIGHT CTRL",
        "AltLeft" => "LEFT ALT",
        "AltRight" => "RIGHT ALT",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Slash" => "/",
        "Comma" => ",",
        "Period" => ".",
        "Semicolon" => ";",
        "Quote" => "'",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        "ArrowUp" => "UP",
        "ArrowDown" => "DOWN",
        "ArrowLeft" => "LEFT ARROW",
        "ArrowRight" => "RIGHT ARROW",
        "PageUp" => "PAGE UP",
        "PageDown" => "PAGE DOWN",
        "CapsLock" => "CAPS LOCK",
        other => return other.to_uppercase(),
    };
    label.to_string()
}
