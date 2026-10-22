// SPDX-License-Identifier: MIT OR Apache-2.0

//! Translation of winit input events into [`umicraft_ui::UiInput`].
//!
//! The UI crate deliberately knows nothing about winit (ADR-0006). This module
//! is the only place where a winit key code, mouse button, wheel delta or
//! modifier state becomes a [`umicraft_ui::InputEvent`]. The windowed client
//! feeds the resulting events to the UI only while the debug screen is open;
//! the game input path is untouched when it is closed.
//!
//! The translator keeps only the two pieces of input state that winit does not
//! repeat in every event: the pointer position (`WindowEvent::MouseInput`
//! carries no position) and the modifier state. It never touches the game
//! thread, the network or the UI context itself.
//!
//! Wheel direction: winit documents positive line deltas as "content moves
//! down", while [`umicraft_ui::InputEvent::Wheel`] uses positive for scrolling
//! down. The vertical delta is therefore negated.

use umicraft_ui::{InputEvent, Key, Modifiers, PointerButton, UiInput};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{KeyCode, ModifiersState};

/// Approximate number of pixels in one wheel line for pixel-precision
/// touchpads, used to keep the list scrolling at a familiar speed.
const PIXELS_PER_LINE: f32 = 40.0;

/// Maps a winit physical key to the UI key vocabulary.
///
/// Only keys with a positional or editing meaning are mapped. Printable
/// characters arrive through [`InputEvent::Text`] instead, and keys the M4
/// screen does not use (including F2, which the window handles itself) return
/// `None`.
#[must_use]
pub fn ui_key(code: KeyCode) -> Option<Key> {
    match code {
        KeyCode::Tab => Some(Key::Tab),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Escape => Some(Key::Escape),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Delete => Some(Key::Delete),
        KeyCode::ArrowLeft => Some(Key::ArrowLeft),
        KeyCode::ArrowRight => Some(Key::ArrowRight),
        KeyCode::ArrowUp => Some(Key::ArrowUp),
        KeyCode::ArrowDown => Some(Key::ArrowDown),
        KeyCode::Home => Some(Key::Home),
        KeyCode::End => Some(Key::End),
        KeyCode::PageUp => Some(Key::PageUp),
        KeyCode::PageDown => Some(Key::PageDown),
        _ => None,
    }
}

/// Translates one winit key state change into a UI key event.
///
/// Returns `None` for keys the UI does not map. Auto-repeat is not filtered
/// here: the window decides which keys must fire once (F2) and lets editing
/// keys repeat naturally.
#[must_use]
pub fn key_event(code: KeyCode, state: ElementState) -> Option<InputEvent> {
    let key = ui_key(code)?;
    match state {
        ElementState::Pressed => Some(InputEvent::KeyPressed(key)),
        ElementState::Released => Some(InputEvent::KeyReleased(key)),
    }
}

/// Translates a winit text payload into a UI text event.
///
/// Control characters are removed so that an Enter key or an IME sequence
/// cannot inject a newline into the single-line text field. Returns `None`
/// when nothing printable remains.
#[must_use]
pub fn text_event(text: &str) -> Option<InputEvent> {
    let text: String = text
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    (!text.is_empty()).then_some(InputEvent::Text(text))
}

/// Converts the winit modifier state into the UI modifier flags.
#[must_use]
pub fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        control: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}

/// Translates one winit mouse button state change at `position`.
///
/// Buttons the UI does not track (back/forward and the platform extras)
/// return `None`. The caller supplies the pointer position because winit's
/// `MouseInput` event does not carry one.
#[must_use]
pub fn button_event(
    state: ElementState,
    button: MouseButton,
    position: [f32; 2],
) -> Option<InputEvent> {
    let button = match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        _ => return None,
    };
    match state {
        ElementState::Pressed => Some(InputEvent::PointerPressed { position, button }),
        ElementState::Released => Some(InputEvent::PointerReleased { position, button }),
    }
}

/// Translates a winit wheel delta into one UI wheel event.
///
/// Only the vertical component is used. Positive UI deltas mean "scroll down";
/// winit's positive component means "the content moves down", so the sign is
/// flipped (see the module docs).
#[must_use]
pub fn wheel_event(delta: MouseScrollDelta) -> Option<InputEvent> {
    let lines = match delta {
        MouseScrollDelta::LineDelta(_, vertical) => -vertical,
        MouseScrollDelta::PixelDelta(position) => -(position.y as f32) / PIXELS_PER_LINE,
    };
    (lines.is_finite() && lines != 0.0).then_some(InputEvent::Wheel { delta: lines })
}

/// Accumulates winit events between two UI frames.
///
/// The window owns one translator on its render thread. It records the
/// modifier and pointer state that later events need and queues the converted
/// [`InputEvent`]s; [`Self::take`] moves the queue into the UI context.
#[derive(Debug, Clone, Default)]
pub struct UiInputTranslator {
    events: Vec<InputEvent>,
    modifiers: Modifiers,
    pointer: [f32; 2],
}

impl UiInputTranslator {
    /// Creates an empty translator at the origin with no modifiers held.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the modifier state used by the next frame.
    pub fn modifiers(&mut self, state: ModifiersState) {
        self.modifiers = modifiers(state);
    }

    /// Records the pointer position and queues a pointer-moved event.
    pub fn pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        self.pointer = [position.x as f32, position.y as f32];
        self.events.push(InputEvent::PointerMoved {
            position: self.pointer,
        });
    }

    /// Queues a pointer-button event at the most recent pointer position.
    pub fn pointer_button(&mut self, state: ElementState, button: MouseButton) {
        if let Some(event) = button_event(state, button, self.pointer) {
            self.events.push(event);
        }
    }

    /// Queues a wheel event.
    pub fn wheel(&mut self, delta: MouseScrollDelta) {
        if let Some(event) = wheel_event(delta) {
            self.events.push(event);
        }
    }

    /// Queues the key event and, for printable presses, the text it produced.
    ///
    /// The window passes the text payload of the winit key event. Keeping it
    /// here preserves the key/text order of one press.
    pub fn key(&mut self, code: KeyCode, state: ElementState, text: Option<&str>) {
        if let Some(event) = key_event(code, state) {
            self.events.push(event);
        }
        if state == ElementState::Pressed {
            if let Some(text) = text {
                self.text(text);
            }
        }
    }

    /// Queues a text payload, filtering control characters.
    pub fn text(&mut self, text: &str) {
        if let Some(event) = text_event(text) {
            self.events.push(event);
        }
    }

    /// Moves the queued events and the current modifiers into a UI frame.
    pub fn take(&mut self) -> UiInput {
        UiInput {
            events: std::mem::take(&mut self.events),
            modifiers: self.modifiers,
        }
    }

    /// Drops the queued events without touching the modifier or pointer state.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::ModifiersState;

    #[test]
    fn key_down_and_up_are_translated() {
        let mut translator = UiInputTranslator::new();
        translator.key(KeyCode::Backspace, ElementState::Pressed, None);
        translator.key(KeyCode::Backspace, ElementState::Released, None);
        let input = translator.take();
        assert_eq!(
            input.events,
            vec![
                InputEvent::KeyPressed(Key::Backspace),
                InputEvent::KeyReleased(Key::Backspace),
            ]
        );
        assert!(translator.take().events.is_empty());
    }

    #[test]
    fn a_printable_press_carries_key_and_text() {
        let mut translator = UiInputTranslator::new();
        translator.key(KeyCode::KeyA, ElementState::Pressed, Some("a"));
        assert_eq!(
            translator.take().events,
            vec![InputEvent::Text("a".to_string())]
        );

        // F2 is the window's toggle, not a UI key, so only text (none here)
        // would remain.
        assert_eq!(ui_key(KeyCode::F2), None);
    }

    #[test]
    fn text_filters_control_characters() {
        assert_eq!(
            text_event("a\nb\tc"),
            Some(InputEvent::Text("abc".to_string()))
        );
        assert_eq!(text_event("\r\n"), None);
        assert_eq!(text_event(""), None);
    }

    #[test]
    fn pointer_move_press_and_release_keep_the_position() {
        let mut translator = UiInputTranslator::new();
        translator.pointer_moved(PhysicalPosition::new(12.5, 34.0));
        translator.pointer_button(ElementState::Pressed, MouseButton::Left);
        translator.pointer_button(ElementState::Released, MouseButton::Left);
        translator.pointer_button(ElementState::Pressed, MouseButton::Back);

        let input = translator.take();
        assert_eq!(
            input.events,
            vec![
                InputEvent::PointerMoved {
                    position: [12.5, 34.0],
                },
                InputEvent::PointerPressed {
                    position: [12.5, 34.0],
                    button: PointerButton::Left,
                },
                InputEvent::PointerReleased {
                    position: [12.5, 34.0],
                    button: PointerButton::Left,
                },
            ]
        );
    }

    #[test]
    fn wheel_lines_are_flipped_and_pixels_are_converted() {
        assert_eq!(
            wheel_event(MouseScrollDelta::LineDelta(0.0, 1.0)),
            Some(InputEvent::Wheel { delta: -1.0 })
        );
        assert_eq!(
            wheel_event(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                0.0, 80.0
            ))),
            Some(InputEvent::Wheel { delta: -2.0 })
        );
        assert_eq!(wheel_event(MouseScrollDelta::LineDelta(0.0, 0.0)), None);
    }

    #[test]
    fn modifiers_are_copied_into_the_frame() {
        let mut translator = UiInputTranslator::new();
        translator.modifiers(ModifiersState::SHIFT | ModifiersState::CONTROL);
        let input = translator.take();
        assert!(input.events.is_empty());
        assert!(input.modifiers.shift);
        assert!(input.modifiers.control);
        assert!(!input.modifiers.alt);
        assert!(!input.modifiers.meta);
    }

    #[test]
    fn clear_drops_events_but_keeps_the_pointer() {
        let mut translator = UiInputTranslator::new();
        translator.pointer_moved(PhysicalPosition::new(5.0, 6.0));
        translator.clear();
        translator.pointer_button(ElementState::Pressed, MouseButton::Middle);
        assert_eq!(
            translator.take().events,
            vec![InputEvent::PointerPressed {
                position: [5.0, 6.0],
                button: PointerButton::Middle,
            }]
        );
    }
}
