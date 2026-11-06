// SPDX-License-Identifier: MIT OR Apache-2.0

//! Styled text runs drawn the way vanilla `FontRenderer.renderStringAtPos`
//! draws them: shadow, bold, underline, strikethrough and obfuscated text.
//!
//! Source: `net.minecraft.client.gui.FontRenderer` of MCP 1.8 (ADR-0001).
//! This is the single implementation shared by the chat and any other
//! formatted-text consumer of the HUD.
//!
//! # Limitation
//!
//! Italic is accepted but drawn upright. Vanilla shears the glyph quad by one
//! pixel at the top, and [`DrawCommand::Text`] only describes axis-aligned
//! strings, so a faithful shear needs a skewed-quad command in the draw list.

use std::borrow::Cow;

use crate::draw::{Color, DrawList, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;

/// Height of a font line in GUI units (`FontRenderer.FONT_HEIGHT`).
const FONT_HEIGHT: f32 = 9.0;
/// The shadow keeps a quarter of the brightness (`(c & 0xFCFCFC) >> 2`).
const SHADOW_BRIGHTNESS: f32 = 0.25;

/// One run of text sharing a colour and a set of style flags.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledRun {
    /// The text of the run.
    pub text: String,
    /// Straight-alpha RGBA colour of the text.
    pub color: Color,
    /// Draw each glyph twice, one unit apart, and widen it by one unit.
    pub bold: bool,
    /// Requested italic; currently drawn upright (see the module docs).
    pub italic: bool,
    /// Draw a line under the run.
    pub underlined: bool,
    /// Draw a line through the run.
    pub strikethrough: bool,
    /// Replace every glyph by a random glyph of the same width each tick.
    pub obfuscated: bool,
}

impl StyledRun {
    /// A run without any style flag.
    #[must_use]
    pub fn plain(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
            bold: false,
            italic: false,
            underlined: false,
            strikethrough: false,
            obfuscated: false,
        }
    }
}

/// The advance of `runs` at `scale`, in pixels.
///
/// Bold glyphs are one unit wider. Obfuscated glyphs keep the width of the
/// glyph they replace, so the result does not depend on the tick.
#[must_use]
pub fn styled_width(font: &FontAtlas, runs: &[StyledRun], scale: f32) -> f32 {
    runs.iter().map(|run| run_width(font, run)).sum::<f32>() * scale
}

/// Draws `runs` starting at `at` (top-left, in pixels) and returns the advance.
///
/// `scale` is the number of pixels per GUI unit. With `shadow` every run is
/// first drawn one unit down and right at a quarter of its brightness, as
/// `drawStringWithShadow` does. `tick` drives the obfuscated glyphs: the same
/// tick always yields the same glyphs, a new tick yields new ones.
///
/// ```
/// use umicraft_ui::styled_text::{draw_styled, StyledRun};
/// use umicraft_ui::{DrawList, FontAtlas};
///
/// let font = FontAtlas::fallback();
/// let mut list = DrawList::new();
/// let runs = [StyledRun::plain("hi", [1.0; 4])];
/// let advance = draw_styled(&mut list, &font, [0.0, 0.0], &runs, 2.0, true, 0);
/// assert!(advance > 0.0);
/// ```
pub fn draw_styled(
    list: &mut DrawList,
    font: &FontAtlas,
    at: [f32; 2],
    runs: &[StyledRun],
    scale: f32,
    shadow: bool,
    tick: u32,
) -> f32 {
    if shadow {
        draw_pass(list, font, at, runs, scale, Pass::Shadow, tick);
    }
    draw_pass(list, font, at, runs, scale, Pass::Text, tick)
}

/// Which of the two passes of `drawStringWithShadow` is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// The text itself.
    Text,
    /// The shadow: a quarter of the brightness, one unit down and right.
    Shadow,
}

/// Draws every run once; the shadow pass dims the colour and offsets the glyphs.
///
/// Returns how far the pen moved, which is what drawing advances and not
/// what [`styled_width`] measures (`renderStringAtPos` against
/// `getStringWidth`).
fn draw_pass(
    list: &mut DrawList,
    font: &FontAtlas,
    at: [f32; 2],
    runs: &[StyledRun],
    scale: f32,
    pass: Pass,
    tick: u32,
) -> f32 {
    let brightness = match pass {
        Pass::Text => 1.0,
        Pass::Shadow => SHADOW_BRIGHTNESS,
    };
    let mut x = at[0];
    let mut glyph_index = 0_u64;
    for run in runs {
        let color = [
            run.color[0] * brightness,
            run.color[1] * brightness,
            run.color[2] * brightness,
            run.color[3],
        ];
        draw_run(
            list,
            font,
            [x, at[1]],
            run,
            scale,
            color,
            ((tick, glyph_index), pass),
        );
        glyph_index += run.text.chars().count() as u64;
        x += drawn_width(font, run) * scale;
    }
    x - at[0]
}

/// Draws one run and its decoration lines at `at`.
fn draw_run(
    list: &mut DrawList,
    font: &FontAtlas,
    at: [f32; 2],
    run: &StyledRun,
    scale: f32,
    color: Color,
    ((tick, first_glyph), pass): ((u32, u64), Pass),
) {
    let text: Cow<'_, str> = if run.obfuscated {
        Cow::Owned(scramble(font, &run.text, tick, first_glyph))
    } else {
        Cow::Borrowed(&run.text)
    };
    let style = TextStyle::new(scale, color);
    let shadow = pass == Pass::Shadow;
    // The glyphs of the shadow are one unit down and right, except the
    // Unicode ones, which `renderStringAtPos` pulls back onto the glyph.
    let shifted = if shadow {
        [at[0] + scale, at[1] + scale]
    } else {
        at
    };
    let unicode_shadow = shadow && text.chars().any(|character| font.is_unicode(character));
    if run.bold || unicode_shadow {
        draw_glyphs(list, font, (at, shadow), &text, (style, run.bold));
    } else if !text.is_empty() {
        list.text(shifted, text.as_ref(), style);
    }
    let at = shifted;

    let width = drawn_width(font, run) * scale;
    if run.strikethrough {
        list.rect(
            Rect::new(
                at[0],
                at[1] + (FONT_HEIGHT / 2.0).floor() * scale,
                width,
                scale,
            ),
            color,
        );
    }
    if run.underlined {
        // Vanilla starts the underline one unit left of the first glyph.
        list.rect(
            Rect::new(
                at[0] - scale,
                at[1] + (FONT_HEIGHT - 1.0) * scale,
                width + scale,
                scale,
            ),
            color,
        );
    }
}

/// Draws a run glyph by glyph, for what a whole string cannot say: a bold
/// glyph is drawn twice and advances one extra unit, and the shadow of a
/// Unicode glyph is not offset (`renderStringAtPos`: `flag` takes the glyph
/// one unit back before it is drawn, which cancels the shadow's own offset).
fn draw_glyphs(
    list: &mut DrawList,
    font: &FontAtlas,
    (at, shadow): ([f32; 2], bool),
    text: &str,
    (style, bold): (TextStyle, bool),
) {
    let mut x = at[0];
    for character in text.chars() {
        let advance = font.pen_advance(character);
        if advance > 0.0 {
            let offset = if shadow && !font.is_unicode(character) {
                style.scale
            } else {
                0.0
            };
            let at = [x + offset, at[1] + offset];
            list.text(at, character, style);
            if bold {
                list.text([at[0] + style.scale, at[1]], character, style);
            }
        }
        x += (advance + if bold { 1.0 } else { 0.0 }) * style.scale;
    }
}

/// The advance of one run in GUI units, as `getStringWidth` measures it.
fn run_width(font: &FontAtlas, run: &StyledRun) -> f32 {
    let bold = if run.bold { 1.0 } else { 0.0 };
    run.text
        .chars()
        .map(|character| font.advance(character, 1.0) + bold)
        .sum()
}

/// How far drawing one run moves the pen, in GUI units: `(int) f` per glyph
/// plus the extra unit of a bold one.
fn drawn_width(font: &FontAtlas, run: &StyledRun) -> f32 {
    let bold = if run.bold { 1.0 } else { 0.0 };
    run.text
        .chars()
        .map(|character| font.pen_advance(character) + bold)
        .sum()
}

/// Replaces every printable glyph of `text` by one of the same advance.
fn scramble(font: &FontAtlas, text: &str, tick: u32, first_glyph: u64) -> String {
    text.chars()
        .zip(first_glyph..)
        .map(|(character, index)| scrambled_glyph(font, character, mix(tick, index)))
        .collect()
}

/// A glyph with the advance of `character` picked by `seed`, or `character`
/// itself when it is blank or the font has no same-width glyph.
fn scrambled_glyph(font: &FontAtlas, character: char, seed: u64) -> char {
    // `randomStyle && j != -1`: only the characters of the 1.8 grid are
    // scrambled, and only into other characters of the grid.
    if character.is_whitespace() || character.is_control() || font.is_unicode(character) {
        return character;
    }
    let advance = font.advance(character, 1.0);
    let same_width = || {
        font.grid_glyphs().iter().filter(move |glyph| {
            !glyph.character.is_whitespace() && (glyph.advance - advance).abs() < f32::EPSILON
        })
    };
    let count = same_width().count();
    if count == 0 {
        return character;
    }
    same_width()
        .nth((seed % count as u64) as usize)
        .map_or(character, |glyph| glyph.character)
}

/// A small integer hash (SplitMix64 finaliser) of the tick and glyph index.
fn mix(tick: u32, index: u64) -> u64 {
    let mut z = (u64::from(tick) << 32) ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCommand;

    const WHITE: Color = [1.0; 4];

    fn texts(list: &DrawList) -> Vec<String> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn advance_matches_the_font_measure() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let runs = [StyledRun::plain("ab", WHITE), StyledRun::plain("cd", WHITE)];
        let advance = draw_styled(&mut list, &font, [0.0; 2], &runs, 2.0, false, 0);
        assert!((advance - font.measure("abcd", 2.0)).abs() < 1e-4);
        assert!((styled_width(&font, &runs, 2.0) - advance).abs() < 1e-4);
    }

    #[test]
    fn bold_adds_one_unit_per_glyph() {
        let font = FontAtlas::fallback();
        let mut run = StyledRun::plain("abc", WHITE);
        run.bold = true;
        let plain = font.measure("abc", 1.0);
        assert!((styled_width(&font, &[run], 1.0) - (plain + 3.0)).abs() < 1e-4);
    }

    #[test]
    fn shadow_is_drawn_first_at_a_quarter_brightness() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let runs = [StyledRun::plain("a", [0.8, 0.4, 0.2, 0.5])];
        draw_styled(&mut list, &font, [10.0, 20.0], &runs, 2.0, true, 0);
        match (&list.commands()[0], &list.commands()[1]) {
            (
                DrawCommand::Text {
                    position: s,
                    style: ss,
                    ..
                },
                DrawCommand::Text {
                    position: m,
                    style: ms,
                    ..
                },
            ) => {
                assert_eq!((*s, *m), ([12.0, 22.0], [10.0, 20.0]));
                assert!((ss.color[0] - 0.2).abs() < 1e-6);
                assert!((ss.color[3] - 0.5).abs() < 1e-6);
                assert_eq!(ms.color, [0.8, 0.4, 0.2, 0.5]);
            }
            other => panic!("unexpected commands {other:?}"),
        }
    }

    #[test]
    fn bold_draws_every_glyph_twice() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let mut run = StyledRun::plain("ab", WHITE);
        run.bold = true;
        draw_styled(&mut list, &font, [0.0; 2], &[run], 1.0, false, 0);
        assert_eq!(texts(&list), ["a", "a", "b", "b"]);
    }

    #[test]
    fn underline_and_strikethrough_are_rectangles() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let mut run = StyledRun::plain("ab", WHITE);
        run.underlined = true;
        run.strikethrough = true;
        draw_styled(&mut list, &font, [0.0; 2], &[run], 1.0, false, 0);
        let rects = list
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Rect { .. }))
            .count();
        assert_eq!(rects, 2);
    }

    #[test]
    fn obfuscated_width_is_stable_across_ticks() {
        let font = FontAtlas::fallback();
        let mut run = StyledRun::plain("hello world", WHITE);
        run.obfuscated = true;
        let widths: Vec<f32> = (0..8)
            .map(|tick| {
                let mut list = DrawList::new();
                draw_styled(
                    &mut list,
                    &font,
                    [0.0; 2],
                    std::slice::from_ref(&run),
                    1.0,
                    false,
                    tick,
                )
            })
            .collect();
        assert!(widths.iter().all(|w| (w - widths[0]).abs() < 1e-4));
        assert!((widths[0] - font.measure("hello world", 1.0)).abs() < 1e-4);
    }

    #[test]
    fn obfuscated_glyphs_are_deterministic_per_tick_and_change_over_ticks() {
        let font = FontAtlas::fallback();
        let mut run = StyledRun::plain("abcdefgh", WHITE);
        run.obfuscated = true;
        let draw = |tick| {
            let mut list = DrawList::new();
            draw_styled(
                &mut list,
                &font,
                [0.0; 2],
                std::slice::from_ref(&run),
                1.0,
                false,
                tick,
            );
            texts(&list)
        };
        assert_eq!(draw(3), draw(3));
        assert!((0..16).any(|tick| draw(tick) != draw(3)));
    }
}
