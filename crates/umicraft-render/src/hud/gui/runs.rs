// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers shared by the overlay drawers: styled runs from views, their
//! width, the obfuscation tick and the line wrapping of the tab list header.
//!
//! Wrapping follows MCP `FontRenderer.listFormattedStringToWidth`: it breaks
//! at the last space that fits, at an explicit newline, or inside a word that
//! is wider than the line.

use umicraft_ui::styled_text::{draw_styled, styled_width, StyledRun};
use umicraft_ui::{DrawList, FontAtlas};

use crate::hud::{ChatRunView, HudState};

/// The runs as styled runs with `alpha`, optionally all italic
/// (`EnumChatFormatting.ITALIC + text`).
pub(super) fn styled(runs: &[ChatRunView], alpha: f32, italic: bool) -> Vec<StyledRun> {
    let channel = |value: u8| f32::from(value) / 255.0;
    runs.iter()
        .map(|run| StyledRun {
            text: run.text.clone(),
            color: [
                channel(run.color[0]),
                channel(run.color[1]),
                channel(run.color[2]),
                alpha,
            ],
            bold: run.bold,
            italic: run.italic || italic,
            underlined: run.underlined,
            strikethrough: run.strikethrough,
            obfuscated: run.obfuscated,
        })
        .collect()
}

/// The width of the runs in GUI units (`FontRenderer.getStringWidth`).
pub(super) fn width(font: &FontAtlas, runs: &[ChatRunView]) -> f32 {
    styled_width(font, &styled(runs, 1.0, false), 1.0)
}

/// Draws `runs` at `at` (in pixels) with `alpha` and the shadow of
/// `drawStringWithShadow` (or none for `drawString`).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_runs(
    list: &mut DrawList,
    font: &FontAtlas,
    state: &HudState,
    at: [f32; 2],
    runs: &[ChatRunView],
    scale: f32,
    alpha: f32,
    shadow: bool,
) -> f32 {
    draw_styled(
        list,
        font,
        at,
        &styled(runs, alpha, false),
        scale,
        shadow,
        obfuscation_tick(state),
    )
}

/// Obfuscated glyphs change every 50 ms.
pub(super) fn obfuscation_tick(state: &HudState) -> u32 {
    state
        .now
        .map_or(0, |now| (now.elapsed().as_millis() / 50) as u32)
}

/// Splits `runs` into rows no wider than `max_width`.
pub(super) fn wrap(
    font: &FontAtlas,
    runs: &[ChatRunView],
    max_width: f32,
) -> Vec<Vec<ChatRunView>> {
    let glyphs: Vec<(char, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| run.text.chars().map(move |c| (c, index)))
        .collect();
    let advance = |(character, index): (char, usize)| {
        font.advance(character, 1.0) + if runs[index].bold { 1.0 } else { 0.0 }
    };

    let mut rows = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let mut end = start;
        let mut row_width = 0.0;
        let mut newline = false;
        while end < glyphs.len() {
            if glyphs[end].0 == '\n' {
                newline = true;
                break;
            }
            let next = row_width + advance(glyphs[end]);
            if next > max_width && end > start {
                break;
            }
            row_width = next;
            end += 1;
        }
        let (row_end, next_start) = if newline || end == glyphs.len() {
            (end, end + usize::from(newline))
        } else {
            match (start..=end).rev().find(|&i| glyphs[i].0 == ' ') {
                Some(space) => (space, space + 1),
                None => (end, end),
            }
        };
        rows.push(regroup(runs, &glyphs[start..row_end]));
        start = next_start;
    }
    rows
}

/// Glyphs back into runs that keep the style of the run they came from.
fn regroup(runs: &[ChatRunView], glyphs: &[(char, usize)]) -> Vec<ChatRunView> {
    let mut out: Vec<(usize, ChatRunView)> = Vec::new();
    for &(character, index) in glyphs {
        match out.last_mut() {
            Some((last, run)) if *last == index => run.text.push(character),
            _ => out.push((
                index,
                ChatRunView {
                    text: character.to_string(),
                    ..runs[index].clone()
                },
            )),
        }
    }
    out.into_iter().map(|(_, run)| run).collect()
}

/// A run of plain white text, for tests.
#[cfg(test)]
pub(super) fn run(text: &str) -> ChatRunView {
    ChatRunView {
        text: text.to_owned(),
        color: [255; 3],
        bold: false,
        italic: false,
        underlined: false,
        strikethrough: false,
        obfuscated: false,
        ..ChatRunView::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::plain_text;

    fn rows(text: &str, max: f32) -> Vec<String> {
        let font = FontAtlas::fallback();
        wrap(&font, &[run(text)], max)
            .iter()
            .map(|row| plain_text(row))
            .collect()
    }

    #[test]
    fn a_short_line_stays_in_one_row() {
        assert_eq!(rows("hello", 500.0), ["hello"]);
    }

    #[test]
    fn a_newline_starts_a_row() {
        assert_eq!(rows("a\nb", 500.0), ["a", "b"]);
        assert_eq!(rows("a\n", 500.0), ["a"]);
    }

    #[test]
    fn a_long_line_breaks_at_a_space_that_fits() {
        let font = FontAtlas::fallback();
        let one = font.measure("aaaa", 1.0);
        let wrapped = rows("aaaa bbbb", one + 1.0);
        assert_eq!(wrapped, ["aaaa", "bbbb"]);
    }

    #[test]
    fn a_word_wider_than_the_row_breaks_inside() {
        let font = FontAtlas::fallback();
        let two = font.measure("aa", 1.0);
        let wrapped = rows("aaaaa", two);
        assert!(
            wrapped.len() >= 2 && wrapped.concat() == "aaaaa",
            "{wrapped:?}"
        );
    }

    #[test]
    fn rows_keep_the_style_of_their_runs() {
        let font = FontAtlas::fallback();
        let mut red = run("red ");
        red.color = [255, 0, 0];
        let wrapped = wrap(&font, &[red, run("white")], 500.0);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].len(), 2);
        assert_eq!(wrapped[0][0].color, [255, 0, 0]);
    }
}
