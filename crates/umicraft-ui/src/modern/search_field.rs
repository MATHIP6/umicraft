// SPDX-License-Identifier: MIT OR Apache-2.0

//! A one-line search box in the modern look. It takes the typed text of the
//! frame whether or not it is focused, because it is the only text input of
//! the screens that use it.

use crate::context::Ui;
use crate::geom::Rect;
use crate::input::{InputEvent, Key};
use crate::text::FontAtlas;

use crate::icons::Icon;

use super::text::{draw_icon, draw_text, fit_text, measure_text, Align};
use super::theme::{RADIUS, TEXT, TEXT_MUTED};

/// The most characters a query can have.
pub const MAX_CHARS: usize = 32;
/// Space between the border and the text, in GUI units.
const INSET: f32 = 4.0;
/// Size of the typed text, in GUI units.
const TEXT_SIZE: f32 = 5.8;
/// Edge of the magnifier, in GUI units.
const ICON_SIZE: f32 = 7.5;
/// Space between the magnifier and the text, in GUI units.
const ICON_GAP: f32 = 3.0;
/// Space between the caret and the placeholder, in GUI units: the caret sits at
/// the start of the text, so the placeholder starts right of it instead of
/// under it.
const PLACEHOLDER_GAP: f32 = 3.0;

/// Applies the input events of a frame to `text`: typed and pasted text is
/// appended (without control characters, up to [`MAX_CHARS`]) and Backspace
/// removes the last character. Returns whether `text` changed.
pub fn edit(text: &mut String, events: &[InputEvent]) -> bool {
    let mut changed = false;
    for event in events {
        match event {
            InputEvent::Text(typed) | InputEvent::Paste(typed) => {
                let room = MAX_CHARS.saturating_sub(text.chars().count());
                let kept = typed.chars().filter(|c| !c.is_control()).take(room);
                let old = text.len();
                text.extend(kept);
                changed |= text.len() != old;
            }
            InputEvent::KeyPressed(Key::Backspace) => changed |= text.pop().is_some(),
            _ => {}
        }
    }
    changed
}

/// Edits `text` with this frame's input and draws the box in `rect` (physical
/// pixels), with `placeholder` in it while empty. Returns whether the text
/// changed. `caret_on` is the blink phase.
pub fn search_field(
    ui: &mut Ui,
    brand: &FontAtlas,
    rect: Rect,
    (text, placeholder): (&mut String, &str),
    (scale, caret_on): (f32, bool),
) -> bool {
    let changed = edit(text, &ui.input().events);
    ui.draw()
        .rounded_rect(rect, RADIUS * scale, [1.0, 1.0, 1.0, 0.05]);
    let size = TEXT_SIZE * scale;
    let middle = rect.y + rect.height * 0.5;
    let icon = (rect.x + (INSET + ICON_SIZE * 0.5) * scale, middle);
    draw_icon(
        ui,
        brand,
        Icon::Search,
        icon,
        (ICON_SIZE * scale, TEXT_MUTED),
    );
    let left = rect.x + (INSET + ICON_SIZE + ICON_GAP) * scale;
    let at = (left, middle);
    if text.is_empty() {
        let beside_caret = (at.0 + PLACEHOLDER_GAP * scale, at.1);
        draw_text(
            ui,
            brand,
            placeholder,
            beside_caret,
            (size, TEXT_MUTED, Align::Left),
        );
    } else {
        let room = rect.right() - INSET * scale - left;
        let shown = fit_text(brand, text, size, room);
        draw_text(ui, brand, &shown, at, (size, TEXT, Align::Left));
    }
    if caret_on {
        let shown_width = if text.is_empty() {
            0.0
        } else {
            measure_text(brand, text, size)
        };
        let x = (left + shown_width).min(rect.right() - INSET * scale);
        let caret = Rect::new(
            x,
            rect.y + rect.height * 0.2,
            scale.max(1.0),
            rect.height * 0.6,
        );
        ui.draw().rect(caret, TEXT);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCommand;
    use crate::input::UiInput;

    /// Regression: the caret of an empty box was drawn on the first letter of
    /// the placeholder.
    #[test]
    fn the_placeholder_starts_right_of_the_caret() {
        let viewport = Rect::new(0.0, 0.0, 400.0, 300.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, UiInput::default());
        let brand = FontAtlas::fallback();
        let rect = Rect::new(10.0, 10.0, 200.0, 20.0);
        let mut text = String::new();
        search_field(&mut ui, &brand, rect, (&mut text, "Search"), (2.0, true));
        let commands = ui.draw().commands();
        let placeholder_x = commands.iter().find_map(|c| match c {
            DrawCommand::Text { position, text, .. } if text == "Search" => Some(position[0]),
            _ => None,
        });
        let caret = commands.iter().rev().find_map(|c| match c {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        });
        let (placeholder_x, caret) = (placeholder_x.expect("placeholder"), caret.expect("caret"));
        assert!(
            placeholder_x >= caret.right(),
            "{placeholder_x} vs {caret:?}"
        );
    }
}
