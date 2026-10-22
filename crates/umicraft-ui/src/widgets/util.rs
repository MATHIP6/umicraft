// SPDX-License-Identifier: MIT OR Apache-2.0

//! Small helpers shared by the widgets that edit state from pointer input.

use crate::context::Ui;
use crate::input::{InputEvent, PointerButton};

/// The position of the last primary press delivered in this frame.
///
/// [`Ui::response`] reports that a press landed on a widget, but it does not
/// expose the press position itself. The text field and the list need it to
/// place the caret and to pick a row, so they read the same event stream the
/// context used for its hit test. The last press wins because
/// [`crate::Ui::begin_frame`] also tracks the most recent press.
pub(crate) fn last_primary_press(ui: &Ui) -> Option<[f32; 2]> {
    ui.input()
        .events
        .iter()
        .rev()
        .find_map(|event| match event {
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            } => Some(*position),
            _ => None,
        })
}
