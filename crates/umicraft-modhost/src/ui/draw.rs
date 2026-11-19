// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the `hud_*` host functions do to a widget's content, with their validation.
//!
//! The `ffi` module decodes the raw arguments (pointer, UTF-8) and calls these safe
//! functions. A mod cannot make the HUD misbehave from here: numbers must be finite,
//! sizes non-negative, opacity and text scale are clamped, text is cut to a
//! bounded length and a widget has a bounded number of items (ADR-0086, F3.4).

use umicraft_builtin_mods::HudContent;
use umicraft_modapi_sys::UmiStatus;

use super::{MAX_HUD_ITEMS, MAX_HUD_TEXT_BYTES};

/// Largest size a widget may reserve, in GUI pixels.
const MAX_RESERVE: f32 = 4096.0;
/// Range of the scale of a free label, so a mod cannot draw invisible or screen-size text.
const LABEL_SCALE: (f32, f32) = (0.25, 4.0);

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

/// Keeps `0xRRGGBB`; the alpha byte a C mod may leave in the high bits is dropped.
fn rgb(color: u32) -> u32 {
    color & 0x00ff_ffff
}

/// Cuts `text` to at most [`MAX_HUD_TEXT_BYTES`] without splitting a character.
fn clip(text: &str) -> &str {
    if text.len() <= MAX_HUD_TEXT_BYTES {
        return text;
    }
    let mut end = MAX_HUD_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `OK` while `used` items leave room for one more, `DENIED` otherwise.
fn room(used: usize) -> Result<(), UmiStatus> {
    if used < MAX_HUD_ITEMS {
        Ok(())
    } else {
        Err(UmiStatus::DENIED)
    }
}

fn status(result: Result<(), UmiStatus>) -> UmiStatus {
    result.err().unwrap_or(UmiStatus::OK)
}

/// `hud_line`: adds a text line.
pub(crate) fn line(content: &mut HudContent, text: &str, color: u32) -> UmiStatus {
    status(room(content.lines().len()).map(|()| {
        content.line(rgb(color)).push_str(clip(text));
    }))
}

/// `hud_label`: adds free text centred on `(x, y)`.
pub(crate) fn label(
    content: &mut HudContent,
    at: [f32; 2],
    text: &str,
    color: u32,
    alpha: f32,
    scale: f32,
) -> UmiStatus {
    if !all_finite(&[at[0], at[1], alpha, scale]) {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.labels().len()).map(|()| {
        let scale = scale.clamp(LABEL_SCALE.0, LABEL_SCALE.1);
        let text_out = content.label(at, rgb(color), alpha.clamp(0.0, 1.0), scale);
        text_out.push_str(clip(text));
    }))
}

/// `hud_cell`: adds a boxed cell; `rect` is `[x, y, width, height]`.
pub(crate) fn cell(
    content: &mut HudContent,
    rect: [f32; 4],
    pressed: bool,
    text: &str,
    color: u32,
) -> UmiStatus {
    if !all_finite(&rect) || rect[2] < 0.0 || rect[3] < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.cells().len()).map(|()| {
        content.cell(rect, pressed, rgb(color)).push_str(clip(text));
    }))
}

/// `hud_tick`: adds a filled rectangle; `rect` is `[x, y, width, height]`.
pub(crate) fn tick(content: &mut HudContent, rect: [f32; 4], color: u32, alpha: f32) -> UmiStatus {
    if !all_finite(&[rect[0], rect[1], rect[2], rect[3], alpha]) || rect[2] < 0.0 || rect[3] < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.ticks().len()).map(|()| {
        content.tick(rect, rgb(color), alpha.clamp(0.0, 1.0));
    }))
}

/// `hud_reserve`: keeps the widget at least `width` x `height` pixels big.
pub(crate) fn reserve(content: &mut HudContent, width: f32, height: f32) -> UmiStatus {
    if !all_finite(&[width, height]) || width < 0.0 || height < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    content.reserve = [width.min(MAX_RESERVE), height.min(MAX_RESERVE)];
    UmiStatus::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_recorded_with_masked_colours_and_clamped_opacity() {
        let mut c = HudContent::default();
        assert_eq!(line(&mut c, "fps", 0xff_12_34_56), UmiStatus::OK);
        assert_eq!(
            (c.lines()[0].text.as_str(), c.lines()[0].color),
            ("fps", 0x12_34_56)
        );
        label(&mut c, [1.0, 2.0], "x", 0xffffff, 5.0, 100.0);
        assert_eq!(
            (c.labels()[0].alpha, c.labels()[0].scale),
            (1.0, LABEL_SCALE.1)
        );
        cell(&mut c, [0.0, 0.0, 10.0, 10.0], true, "W", 1);
        assert!(c.cells()[0].pressed);
        tick(&mut c, [0.0, 0.0, 1.0, 1.0], 2, -1.0);
        assert_eq!(c.ticks()[0].alpha, 0.0);
        assert_eq!(reserve(&mut c, 50.0, 99999.0), UmiStatus::OK);
        assert_eq!(c.reserve, [50.0, MAX_RESERVE]);
    }

    #[test]
    fn non_finite_and_negative_numbers_are_refused() {
        let mut c = HudContent::default();
        let bad = UmiStatus::INVALID_ARGUMENT;
        assert_eq!(label(&mut c, [f32::NAN, 0.0], "x", 0, 1.0, 1.0), bad);
        assert_eq!(label(&mut c, [0.0, 0.0], "x", 0, f32::INFINITY, 1.0), bad);
        assert_eq!(cell(&mut c, [0.0, 0.0, -1.0, 1.0], false, "x", 0), bad);
        assert_eq!(tick(&mut c, [0.0, 0.0, 1.0, f32::NAN], 0, 1.0), bad);
        assert_eq!(reserve(&mut c, -1.0, 1.0), bad);
        assert_eq!(reserve(&mut c, f32::NAN, 1.0), bad);
        assert!(c.is_empty());
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let mut c = HudContent::default();
        let text = "\u{30c4}".repeat(MAX_HUD_TEXT_BYTES);
        line(&mut c, &text, 0);
        let kept = &c.lines()[0].text;
        assert!(kept.len() <= MAX_HUD_TEXT_BYTES && kept.len() > MAX_HUD_TEXT_BYTES - 3);
    }

    #[test]
    fn a_widget_has_a_bounded_number_of_items() {
        let mut c = HudContent::default();
        for _ in 0..MAX_HUD_ITEMS {
            assert_eq!(line(&mut c, "x", 0), UmiStatus::OK);
            assert_eq!(tick(&mut c, [0.0; 4], 0, 1.0), UmiStatus::OK);
        }
        assert_eq!(line(&mut c, "x", 0), UmiStatus::DENIED);
        assert_eq!(tick(&mut c, [0.0; 4], 0, 1.0), UmiStatus::DENIED);
    }
}
