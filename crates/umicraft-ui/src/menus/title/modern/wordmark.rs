// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Umicraft logo and the "UMICRAFT" wordmark of the modern title screen.
//!
//! The wordmark is set in the brand face with wide tracking, as in the logos
//! of PvP clients. It is drawn letter by letter because the UI has no
//! letter-spacing parameter; eight glyphs per frame cost nothing.

use crate::context::Ui;
use crate::draw::SpriteSheet;
use crate::geom::Rect;
use crate::modern::{draw_text, measure_text, theme, Align};
use crate::text::FontAtlas;

/// The product name of the wordmark.
pub const WORDMARK: &str = "UMICRAFT";
/// The wordmark letters are this many GUI units high.
const WORDMARK_SIZE: f32 = 18.0;
/// Extra space after each letter, as a fraction of the letter size.
const TRACKING: f32 = 0.28;
/// How far a letter rises while it fades in, in GUI units.
const LETTER_RISE: f32 = 6.0;
/// How many letters are on their way in at the same time.
const LETTER_OVERLAP: f32 = 3.0;

/// Draws `logo` fitted into `area`, keeping its aspect ratio and centred, with
/// `alpha` opacity. Nothing is drawn without the sprite.
pub(crate) fn draw_logo(ui: &mut Ui, logo: Option<SpriteSheet>, area: Rect, alpha: f32) {
    let Some(sheet) = logo else {
        return;
    };
    let fit = fitted(area, sheet.width as f32, sheet.height as f32);
    let src = sheet.sub(0, 0, sheet.width, sheet.height);
    ui.draw()
        .image(src, fit, theme::with_opacity(theme::TEXT, alpha));
}

/// Draws the tracked wordmark centred in `band`; `scale` is the GUI scale.
///
/// `reveal` is `0.0` (nothing) to `1.0` (all): the letters come in one after
/// the other, each fading in while it rises into place.
pub(crate) fn draw_wordmark(ui: &mut Ui, brand: &FontAtlas, band: Rect, scale: f32, reveal: f32) {
    let size = WORDMARK_SIZE * scale;
    let gap = size * TRACKING;
    let width = tracked_width(brand, size, gap);
    let mut x = band.x + (band.width - width) * 0.5;
    let y = band.y + band.height * 0.5;
    let letters = WORDMARK.chars().count() as f32;
    let mut buffer = [0u8; 4];
    for (index, letter) in WORDMARK.chars().enumerate() {
        let glyph = letter.encode_utf8(&mut buffer);
        let shown = letter_progress(reveal, index, letters);
        if shown > 0.0 {
            let lift = (1.0 - shown) * LETTER_RISE * scale;
            let color = theme::with_opacity(theme::TEXT, shown);
            draw_text(ui, brand, glyph, (x, y + lift), (size, color, Align::Left));
        }
        x += measure_text(brand, glyph, size) + gap;
    }
}

/// How far letter `index` of `letters` has come when the whole wordmark is
/// `reveal` of the way in: each letter takes [`LETTER_OVERLAP`] letter slots.
fn letter_progress(reveal: f32, index: usize, letters: f32) -> f32 {
    ((reveal * (letters + LETTER_OVERLAP) - index as f32) / LETTER_OVERLAP).clamp(0.0, 1.0)
}

/// The width of the wordmark with `gap` after every letter but the last.
fn tracked_width(brand: &FontAtlas, size: f32, gap: f32) -> f32 {
    let letters = WORDMARK.chars().count() as f32;
    measure_text(brand, WORDMARK, size) + gap * (letters - 1.0)
}

/// The largest rectangle of aspect `width / height` centred in `area`.
fn fitted(area: Rect, width: f32, height: f32) -> Rect {
    if width <= 0.0 || height <= 0.0 {
        return area;
    }
    let factor = (area.width / width).min(area.height / height);
    let (w, h) = (width * factor, height * factor);
    Rect::new(
        area.x + (area.width - w) * 0.5,
        area.y + (area.height - h) * 0.5,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_image_is_fitted_by_its_width_and_centred_vertically() {
        let fit = fitted(Rect::new(0.0, 0.0, 100.0, 100.0), 200.0, 100.0);
        assert_eq!(fit, Rect::new(0.0, 25.0, 100.0, 50.0));
    }

    #[test]
    fn the_letters_come_in_one_after_the_other() {
        let n = 8.0;
        assert_eq!(letter_progress(0.0, 0, n), 0.0);
        assert_eq!(letter_progress(1.0, 7, n), 1.0, "the last letter is in");
        let early = letter_progress(0.3, 0, n);
        let late = letter_progress(0.3, 7, n);
        assert!(early > late, "{early} > {late}");
        assert_eq!(late, 0.0, "the last letter waits for its turn");
    }

    #[test]
    fn an_empty_image_takes_the_whole_area() {
        let area = Rect::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(fitted(area, 0.0, 10.0), area);
    }

    #[test]
    fn tracking_widens_the_wordmark_by_seven_gaps() {
        let atlas = FontAtlas::fallback();
        let plain = measure_text(&atlas, WORDMARK, 48.0);
        assert_eq!(tracked_width(&atlas, 48.0, 2.0), plain + 14.0);
    }
}
