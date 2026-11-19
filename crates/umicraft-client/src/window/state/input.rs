// SPDX-License-Identifier: MIT OR Apache-2.0

//! The keyboard and mouse state the window tracks between two input updates.

use std::collections::HashSet;
use std::time::Instant;

use winit::keyboard::KeyCode;

use crate::input::InputState;

/// What the keyboard and the mouse hold now, and the last input sent.
pub(in crate::window) struct InputTracking {
    pub(in crate::window) keys: HashSet<KeyCode>,
    /// The mouse buttons that are down, by `Binding::Mouse` index.
    pub(in crate::window) buttons: HashSet<u8>,
    pub(in crate::window) mouse_captured: bool,
    /// Whether a Control key is held (word movement in the chat).
    pub(in crate::window) ctrl_held: bool,
    /// Whether Shift is held: a chat click then inserts instead of acting.
    pub(in crate::window) shift_held: bool,
    /// The pointer in physical pixels, for the chat hover and click.
    pub(in crate::window) pointer: (f32, f32),
    /// The mouse motion not yet handed to the game, kept in whole pixels
    /// (ADR-0076).
    pub(in crate::window) mouse: crate::input::mouse_look::MouseSteps,
    pub(in crate::window) last: InputState,
    pub(in crate::window) last_time: Instant,
}
