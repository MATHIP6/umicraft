// SPDX-License-Identifier: MIT OR Apache-2.0

//! Input events consumed by one UI frame.
//!
//! The client translates winit events (or the headless script) into this small
//! backend-agnostic vocabulary. The UI never sees a winit type, so the same
//! tests and the same debug screen run with and without a window.

/// The mouse buttons the UI tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// The primary (usually left) button.
    Left,
    /// The secondary (usually right) button.
    Right,
    /// The middle button.
    Middle,
}

/// Keyboard keys with a UI meaning.
///
/// Printable characters arrive through [`InputEvent::Text`]; this enum only
/// carries the keys whose meaning is positional or editing-related.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Move the focus to the next widget.
    Tab,
    /// Validate the focused widget.
    Enter,
    /// Cancel the focused widget.
    Escape,
    /// Delete the previous character.
    Backspace,
    /// Delete the next character.
    Delete,
    /// Move the caret left.
    ArrowLeft,
    /// Move the caret right.
    ArrowRight,
    /// Move the caret or the selection up.
    ArrowUp,
    /// Move the caret or the selection down.
    ArrowDown,
    /// Move the caret to the start of the line.
    Home,
    /// Move the caret to the end of the line.
    End,
    /// Scroll one page up.
    PageUp,
    /// Scroll one page down.
    PageDown,
}

/// The modifier keys held during an event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub control: bool,
    /// Alt is held.
    pub alt: bool,
    /// The platform "meta" key (Command on macOS, Super on Linux) is held.
    pub meta: bool,
}

impl Modifiers {
    /// True when no modifier is held.
    #[must_use]
    pub fn is_empty(self) -> bool {
        !self.shift && !self.control && !self.alt && !self.meta
    }
}

/// One backend-agnostic input event.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// The pointer moved to a position in physical pixels.
    PointerMoved {
        /// New pointer position.
        position: [f32; 2],
    },
    /// A pointer button was pressed.
    PointerPressed {
        /// Pointer position of the press.
        position: [f32; 2],
        /// Button that was pressed.
        button: PointerButton,
    },
    /// A pointer button was released.
    PointerReleased {
        /// Pointer position of the release.
        position: [f32; 2],
        /// Button that was released.
        button: PointerButton,
    },
    /// The wheel moved by `delta` lines; positive is down.
    Wheel {
        /// Signed number of lines.
        delta: f32,
    },
    /// A non-text key was pressed.
    KeyPressed(Key),
    /// A non-text key was released.
    KeyReleased(Key),
    /// Text was typed; may contain several characters (IME, paste).
    Text(String),
}

/// All input delivered to one UI frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiInput {
    /// Events in arrival order.
    pub events: Vec<InputEvent>,
    /// Modifier state for the frame.
    pub modifiers: Modifiers,
}

impl UiInput {
    /// Creates an input frame from a list of events.
    #[must_use]
    pub fn new(events: Vec<InputEvent>) -> Self {
        Self {
            events,
            modifiers: Modifiers::default(),
        }
    }

    /// True when no event was delivered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{InputEvent, Key, Modifiers, PointerButton, UiInput};

    #[test]
    fn default_input_is_empty() {
        let input = UiInput::default();
        assert!(input.is_empty());
        assert!(input.modifiers.is_empty());
    }

    #[test]
    fn events_keep_their_order() {
        let input = UiInput::new(vec![
            InputEvent::PointerMoved {
                position: [1.0, 2.0],
            },
            InputEvent::PointerPressed {
                position: [1.0, 2.0],
                button: PointerButton::Left,
            },
            InputEvent::KeyPressed(Key::Enter),
        ]);
        assert_eq!(input.events.len(), 3);
        assert_eq!(input.events[2], InputEvent::KeyPressed(Key::Enter));
    }

    #[test]
    fn modifiers_report_their_state() {
        let modifiers = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert!(!modifiers.is_empty());
    }
}
