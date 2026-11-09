// SPDX-License-Identifier: MIT OR Apache-2.0

//! Brand-face text helpers.

use crate::context::Ui;
use crate::draw::{Color, FontFace, TextStyle};
use crate::icons::Icon;
use crate::text::FontAtlas;

use super::theme::BRAND_PIXEL_SIZE;

/// Horizontal anchor of drawn text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// `x` is the left edge.
    Left,
    /// `x` is the centre.
    Center,
    /// `x` is the right edge.
    Right,
}

/// The brand-face style for text whose em is `size_px` physical pixels.
#[must_use]
pub fn brand_text_style(size_px: f32, color: Color) -> TextStyle {
    TextStyle::new(size_px / BRAND_PIXEL_SIZE, color).with_face(FontFace::Brand)
}

/// Width in pixels of `text` at `size_px`, measured with the `brand` atlas.
#[must_use]
pub fn measure_text(brand: &FontAtlas, text: &str, size_px: f32) -> f32 {
    brand.measure(text, size_px / BRAND_PIXEL_SIZE)
}

/// Draws one line of brand text.
///
/// `x` is anchored by `align`; `y` is the vertical centre of the line, so a
/// label can be centred in a button without knowing the font metrics.
pub fn draw_text(
    ui: &mut Ui,
    brand: &FontAtlas,
    text: &str,
    (x, y): (f32, f32),
    (size_px, color, align): (f32, Color, Align),
) {
    let style = brand_text_style(size_px, color);
    let left = match align {
        Align::Left => x,
        Align::Center => x - measure_text(brand, text, size_px) * 0.5,
        Align::Right => x - measure_text(brand, text, size_px),
    };
    let top = y - brand.line_height(style.scale) * 0.5;
    ui.draw().text([left, top], text, style);
}

/// Draws `icon` centred on `at`, `size_px` physical pixels wide, tinted with
/// `color`. The icons live in the brand atlas, so this is a one-letter text
/// run.
pub fn draw_icon(
    ui: &mut Ui,
    brand: &FontAtlas,
    icon: Icon,
    at: (f32, f32),
    (size_px, color): (f32, Color),
) {
    let mut buffer = [0u8; 4];
    let glyph = icon.character().encode_utf8(&mut buffer);
    draw_text(ui, brand, glyph, at, (size_px, color, Align::Center));
}

/// Ellipsis added to text that does not fit.
const ELLIPSIS: &str = "...";

/// `text`, shortened with an ellipsis when it is wider than `max_px` at
/// `size_px`. Text that fits is returned as it is.
#[must_use]
pub fn fit_text(brand: &FontAtlas, text: &str, size_px: f32, max_px: f32) -> String {
    if measure_text(brand, text, size_px) <= max_px {
        return text.to_owned();
    }
    let mut kept: String = text.to_owned();
    while !kept.is_empty() {
        kept.pop();
        let candidate = format!("{}{ELLIPSIS}", kept.trim_end());
        if measure_text(brand, &candidate, size_px) <= max_px {
            return candidate;
        }
    }
    String::new()
}
