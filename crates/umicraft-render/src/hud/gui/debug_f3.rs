// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of the vanilla F3 screen (P9-g).
//!
//! Mirrors MCP 1.8.8 `GuiOverlayDebug.renderDebugInfoLeft` and
//! `renderDebugInfoRight`: each non-empty line gets a `0x90505050` band
//! (`drawRect(x - 1, y - 1, x + width + 1, y + 8, -1873784752)`) behind text
//! of colour `0xE0E0E0` drawn without shadow (`drawString(s, x, y, 14737632)`).
//! Lines are 9 units apart starting at `y = 2`; the left column starts at
//! `x = 2`, the right one ends at `width - 2`. Empty lines draw nothing but
//! keep their row.

use umicraft_ui::{Color, DrawList, FontAtlas, Rect, TextStyle};

use crate::hud::DebugF3View;

/// `FontRenderer.FONT_HEIGHT`, the row pitch of both columns.
const LINE_HEIGHT: f32 = 9.0;
/// The band colour `-1873784752` = `0x90505050` (ARGB).
const BAND: Color = [80.0 / 255.0, 80.0 / 255.0, 80.0 / 255.0, 144.0 / 255.0];
/// The text colour `14737632` = `0xE0E0E0`.
const INK: Color = [224.0 / 255.0, 224.0 / 255.0, 224.0 / 255.0, 1.0];
/// `EnumChatFormatting.GREEN` (`0x55FF55`).
const GREEN: Color = [85.0 / 255.0, 1.0, 85.0 / 255.0, 1.0];
/// `EnumChatFormatting.RED` (`0xFF5555`).
const RED: Color = [1.0, 85.0 / 255.0, 85.0 / 255.0, 1.0];

/// Draws both columns of the F3 screen in GUI units scaled by `sf`.
pub(super) fn draw_debug_f3(
    list: &mut DrawList,
    view: &DebugF3View,
    gw: f32,
    sf: f32,
    font: &FontAtlas,
) {
    for (row, line) in view.left.iter().enumerate() {
        draw_line(list, font, line, row, sf, |_| 2.0);
    }
    for (row, line) in view.right.iter().enumerate() {
        draw_line(list, font, line, row, sf, |width| gw - 2.0 - width);
    }
}

/// Splits a line at the `\u{a7}a` / `\u{a7}c` prefixes into coloured pieces.
///
/// The F3 screen only ever uses green and red (boolean properties); any other
/// code is dropped with its sign, like an unknown code in `FontRenderer`.
fn colour_runs(line: &str) -> Vec<(String, Color)> {
    let mut runs = Vec::new();
    let mut color = INK;
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\u{a7}' {
            current.push(c);
            continue;
        }
        if !current.is_empty() {
            runs.push((std::mem::take(&mut current), color));
        }
        color = match chars.next() {
            Some('a') => GREEN,
            Some('c') => RED,
            _ => INK,
        };
    }
    if !current.is_empty() {
        runs.push((current, color));
    }
    runs
}

/// Draws one row: the band behind it, then its coloured runs.
fn draw_line(
    list: &mut DrawList,
    font: &FontAtlas,
    line: &str,
    row: usize,
    sf: f32,
    left_of: impl Fn(f32) -> f32,
) {
    let runs = colour_runs(line);
    if runs.is_empty() {
        return;
    }
    let width: f32 = runs.iter().map(|(text, _)| font.measure(text, 1.0)).sum();
    let x = left_of(width);
    let y = 2.0 + LINE_HEIGHT * row as f32;
    list.rect(
        Rect::new(
            (x - 1.0) * sf,
            (y - 1.0) * sf,
            (width + 2.0) * sf,
            LINE_HEIGHT * sf,
        ),
        BAND,
    );
    let mut pen = x;
    for (text, color) in &runs {
        list.text([pen * sf, y * sf], text, TextStyle::new(sf, *color));
        pen += font.measure(text, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use umicraft_ui::DrawCommand;

    use super::*;

    fn rects(list: &DrawList) -> Vec<Rect> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Rect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn runs_split_on_colour_codes_and_ignore_the_codes_in_width() {
        let runs = colour_runs("powered: \u{a7}atrue");
        assert_eq!(
            runs,
            vec![("powered: ".to_owned(), INK), ("true".to_owned(), GREEN)]
        );
        assert!(colour_runs("").is_empty());
        assert_eq!(colour_runs("x: \u{a7}cfalse")[1].1, RED);
    }

    #[test]
    fn empty_rows_keep_their_height_but_draw_no_band() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let view = DebugF3View {
            left: vec!["a".into(), String::new(), "b".into()],
            right: Vec::new(),
        };
        draw_debug_f3(&mut list, &view, 400.0, 1.0, &font);
        let bands = rects(&list);
        assert_eq!(bands.len(), 2);
        // Row 0: band from y - 1 = 1, 9 tall, starting at x - 1 = 1.
        assert_eq!((bands[0].x, bands[0].y, bands[0].height), (1.0, 1.0, 9.0));
        // Row 2 starts 18 units lower.
        assert_eq!(bands[1].y, 19.0);
    }

    #[test]
    fn the_right_column_ends_two_units_from_the_edge_and_scales() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let view = DebugF3View {
            left: Vec::new(),
            right: vec!["abc".into()],
        };
        draw_debug_f3(&mut list, &view, 200.0, 2.0, &font);
        let band = rects(&list)[0];
        let width = font.measure("abc", 1.0);
        // The band ends at x + width + 1 = gw - 1, i.e. (gw - 1) * sf.
        assert!((band.x + band.width - 199.0 * 2.0).abs() < 1e-3);
        assert!((band.width - (width + 2.0) * 2.0).abs() < 1e-3);
    }
}
