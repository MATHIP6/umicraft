// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lines of a sign laid out as coloured quads in font pixels (x right,
//! y down), the way `FontRenderer.renderStringAtPos` draws a formatted
//! string (MCP 1.8, ADR-0001).
//!
//! - Each glyph is a quad of the font atlas; an italic one is sheared one
//!   pixel right at the top and one left at the bottom (`renderDefaultChar`).
//! - A bold glyph is drawn twice, the second time one pixel to the right, and
//!   advances one pixel more.
//! - Strikethrough is a one-pixel bar from `y = 3` to `4`, underline one from
//!   `y = 8` to `9` that starts one pixel left of the glyph. Both use the
//!   opaque texel of the atlas.
//! - Obfuscated glyphs are replaced by glyphs of the same width, a new choice
//!   every tick.
//!
//! The colour of a quad is a chat colour index; text without one is black,
//! the colour `0` the sign renderer passes to `drawString`.

use umicraft_game::chat::wrap::wrap;
use umicraft_game::chat::{Color, Run};
use umicraft_ui::styled_text::scramble;
use umicraft_ui::{FontAtlas, Glyph};

/// Chat colour index of text without a colour (black).
const DEFAULT_COLOR: u8 = 0;
/// Top of the strikethrough bar (`FONT_HEIGHT / 2 - 1`).
const STRIKE_TOP: f32 = 3.0;
/// Top of the underline bar (`FONT_HEIGHT - 1`).
const UNDERLINE_TOP: f32 = 8.0;
/// How far an italic glyph leans at its top and bottom edges.
const ITALIC_SHEAR: f32 = 1.0;

/// One quad of a laid-out line, in font pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TextQuad {
    /// Corners in the order top-left, top-right, bottom-right, bottom-left.
    pub corners: [[f32; 2]; 4],
    /// The atlas rectangle `[u0, v0, u1, v1]` the quad samples.
    pub uv: [f32; 4],
    /// The chat colour index, `0..=15`.
    pub color: u8,
}

/// The part of a sign line that is drawn: the first row of the line wrapped
/// to `max_width` pixels (`GuiUtilRenderComponents.func_178908_a(..., 90,
/// ..., false, true).get(0)`).
pub(super) fn first_row(font: &FontAtlas, runs: &[Run], max_width: f32) -> Vec<Run> {
    let advance = |character: char| font.advance(character, 1.0);
    wrap(runs, max_width, &advance)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// The width of `runs` as `getStringWidth` measures it: a bold glyph is one
/// pixel wider.
pub(super) fn width(font: &FontAtlas, runs: &[Run]) -> f32 {
    runs.iter()
        .flat_map(|run| {
            let bold = if run.style.bold { 1.0 } else { 0.0 };
            run.text
                .chars()
                .map(move |character| font.advance(character, 1.0) + bold)
        })
        .sum()
}

/// Appends the quads of `runs` drawn from `(left, top)`. `tick` picks the
/// obfuscated glyphs.
pub(super) fn layout(
    font: &FontAtlas,
    runs: &[Run],
    (left, top): (f32, f32),
    tick: u32,
    out: &mut Vec<TextQuad>,
) {
    let mut pen = left;
    let mut glyph_index = 0_u64;
    for run in runs {
        let text = if run.style.obfuscated {
            scramble(font, &run.text, tick, glyph_index)
        } else {
            run.text.clone()
        };
        glyph_index += run.text.chars().count() as u64;
        let color = run.style.color.map_or(DEFAULT_COLOR, Color::index);
        for character in text.chars() {
            let Some(glyph) = font.glyph_or_fallback(character) else {
                continue;
            };
            pen += draw_char(font, glyph, (pen, top), run, color, out);
        }
    }
}

/// Draws one glyph of `run` with its decorations and returns how far the pen
/// moves (`(int) f`).
fn draw_char(
    font: &FontAtlas,
    glyph: &Glyph,
    (x, top): (f32, f32),
    run: &Run,
    color: u8,
    out: &mut Vec<TextQuad>,
) -> f32 {
    let style = &run.style;
    let mut advance = glyph.pen;
    if !glyph.character.is_whitespace() {
        out.push(glyph_quad(font, glyph, (x, top), style.italic, color));
        if style.bold {
            out.push(glyph_quad(font, glyph, (x + 1.0, top), style.italic, color));
        }
    }
    if style.bold {
        advance += 1.0;
    }
    if style.strikethrough {
        out.push(bar(font, [x, top + STRIKE_TOP, x + advance], color));
    }
    if style.underlined {
        out.push(bar(
            font,
            [x - 1.0, top + UNDERLINE_TOP, x + advance],
            color,
        ));
    }
    advance.trunc()
}

/// The quad of `glyph` with its pen at `(x, y)`.
fn glyph_quad(
    font: &FontAtlas,
    glyph: &Glyph,
    (x, y): (f32, f32),
    italic: bool,
    color: u8,
) -> TextQuad {
    let width = (glyph.uv_max[0] - glyph.uv_min[0]) * font.width as f32 * glyph.scale;
    let height = (glyph.uv_max[1] - glyph.uv_min[1]) * font.height as f32 * glyph.scale;
    let lean = if italic { ITALIC_SHEAR } else { 0.0 };
    TextQuad {
        corners: [
            [x + lean, y],
            [x + width + lean, y],
            [x + width - lean, y + height],
            [x - lean, y + height],
        ],
        uv: [
            glyph.uv_min[0],
            glyph.uv_min[1],
            glyph.uv_max[0],
            glyph.uv_max[1],
        ],
        color,
    }
}

/// A one-pixel bar `[x0, top, x1]` drawn with the opaque texel of the atlas.
fn bar(font: &FontAtlas, [x0, top, x1]: [f32; 3], color: u8) -> TextQuad {
    TextQuad {
        corners: [[x0, top], [x1, top], [x1, top + 1.0], [x0, top + 1.0]],
        uv: font.white_uv,
        color,
    }
}

#[cfg(test)]
mod tests {
    use umicraft_game::chat::Style;

    use super::*;

    fn run(text: &str, style: Style) -> Run {
        Run {
            style,
            ..Run::plain(text)
        }
    }

    fn quads(runs: &[Run]) -> Vec<TextQuad> {
        let font = FontAtlas::fallback();
        let mut out = Vec::new();
        layout(&font, runs, (0.0, 0.0), 0, &mut out);
        out
    }

    #[test]
    fn plain_text_is_black_and_one_quad_per_visible_glyph() {
        let out = quads(&[Run::plain("a b")]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|quad| quad.color == DEFAULT_COLOR));
    }

    #[test]
    fn a_coloured_run_keeps_its_chat_colour() {
        let red = Style {
            color: Color::from_code('c'),
            ..Style::default()
        };
        let out = quads(&[Run::plain("a"), run("b", red)]);
        assert_eq!(out[0].color, 0);
        assert_eq!(out[1].color, 12);
    }

    #[test]
    fn bold_draws_each_glyph_twice_one_pixel_apart_and_advances_one_more() {
        let font = FontAtlas::fallback();
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        let out = quads(&[run("ab", bold)]);
        assert_eq!(out.len(), 4);
        assert_eq!(out[1].corners[0][0] - out[0].corners[0][0], 1.0);
        let step = font.pen_advance('a') + 1.0;
        assert_eq!(out[2].corners[0][0], step);
        assert_eq!(
            width(&font, &[run("ab", bold)]),
            width(&font, &[Run::plain("ab")]) + 2.0
        );
    }

    #[test]
    fn italic_leans_the_top_right_and_the_bottom_left() {
        let italic = Style {
            italic: true,
            ..Style::default()
        };
        let upright = quads(&[Run::plain("a")])[0];
        let leaning = quads(&[run("a", italic)])[0];
        assert!(leaning.corners[0][0] > upright.corners[0][0]);
        assert!(leaning.corners[3][0] < upright.corners[3][0]);
    }

    #[test]
    fn underline_and_strikethrough_are_bars_of_the_opaque_texel() {
        let font = FontAtlas::fallback();
        let both = Style {
            underlined: true,
            strikethrough: true,
            ..Style::default()
        };
        let out = quads(&[run("a", both)]);
        let bars: Vec<&TextQuad> = out.iter().filter(|q| q.uv == font.white_uv).collect();
        assert_eq!(bars.len(), 2);
        let strike = bars[0];
        let underline = bars[1];
        assert_eq!(strike.corners[0], [0.0, STRIKE_TOP]);
        // The underline starts one pixel left of the glyph.
        assert_eq!(underline.corners[0], [-1.0, UNDERLINE_TOP]);
    }

    #[test]
    fn obfuscated_text_keeps_its_width_and_changes_with_the_tick() {
        let font = FontAtlas::fallback();
        let magic = Style {
            obfuscated: true,
            ..Style::default()
        };
        let text = [run("abcdefgh", magic)];
        let mut first = Vec::new();
        let mut later = Vec::new();
        layout(&font, &text, (0.0, 0.0), 1, &mut first);
        layout(&font, &text, (0.0, 0.0), 2, &mut later);
        assert_eq!(first.len(), later.len());
        assert_ne!(first, later);
    }

    #[test]
    fn the_first_row_stops_at_the_width() {
        let font = FontAtlas::fallback();
        let row = first_row(&font, &[Run::plain("a".repeat(100))], 90.0);
        assert!(width(&font, &row) <= 90.0);
        let short = first_row(&font, &[Run::plain("hi")], 90.0);
        assert_eq!(short, [Run::plain("hi")]);
    }
}
