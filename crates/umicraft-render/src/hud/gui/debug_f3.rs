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

use crate::hud::{DebugF3View, LagometerView};

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

/// Draws the one-line Show FPS readout where the F3 screen starts.
pub(super) fn draw_fps_line(list: &mut DrawList, line: &str, sf: f32, font: &FontAtlas) {
    draw_line(list, font, line, 0, sf, |_| 2.0);
}

/// Height of the Lagometer graph in GUI units.
const GRAPH_HEIGHT: f32 = 60.0;
/// Microseconds that fill the graph: 33.3 ms, two 60 Hz frames.
const GRAPH_FULL_US: f32 = 33_333.0;
/// Frame bars of a frame within 60 Hz.
const FRAME_OK: Color = [85.0 / 255.0, 1.0, 85.0 / 255.0, 0.8];
/// Frame bars that missed 60 Hz.
const FRAME_SLOW: Color = [1.0, 85.0 / 255.0, 85.0 / 255.0, 0.8];
/// The sync share of a frame.
const SYNC: Color = [1.0, 1.0, 85.0 / 255.0, 0.9];
/// The mesh upload share of the sync.
const UPLOAD: Color = [85.0 / 255.0, 85.0 / 255.0, 1.0, 0.9];
/// Microseconds of a 60 Hz frame.
const FRAME_60_US: u32 = 16_667;

/// Draws the Lagometer graph in the bottom-left corner: one column per
/// sample, newest on the right, as tall as the timer (full height 33 ms).
/// Each column shows the frame, with the sync and the upload drawn over it.
pub(super) fn draw_lagometer(
    list: &mut DrawList,
    view: &LagometerView,
    gh: f32,
    sf: f32,
    max_columns: usize,
) {
    let columns = view.frame.len().min(max_columns);
    if columns == 0 {
        return;
    }
    let base = gh - 2.0;
    list.rect(
        Rect::new(
            sf,
            (base - GRAPH_HEIGHT - 1.0) * sf,
            (columns as f32 + 2.0) * sf,
            (GRAPH_HEIGHT + 2.0) * sf,
        ),
        BAND,
    );
    let first = view.frame.len() - columns;
    for (column, &frame) in view.frame[first..].iter().enumerate() {
        let x = (2 + column) as f32 * sf;
        let colour = if frame > FRAME_60_US {
            FRAME_SLOW
        } else {
            FRAME_OK
        };
        bar(list, x, base, sf, frame, colour);
        let at = first + column;
        for (timer, colour) in [(&view.sync, SYNC), (&view.upload, UPLOAD)] {
            if let Some(&us) = timer.get(at) {
                bar(list, x, base, sf, us, colour);
            }
        }
    }
}

/// One column of `us` microseconds standing on `base` (GUI units).
fn bar(list: &mut DrawList, x: f32, base: f32, sf: f32, us: u32, colour: Color) {
    let height = (us as f32 / GRAPH_FULL_US).min(1.0) * GRAPH_HEIGHT;
    if height > 0.0 {
        list.rect(Rect::new(x, (base - height) * sf, sf, height * sf), colour);
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
    fn the_lagometer_draws_one_column_per_sample_up_to_the_width() {
        let mut list = DrawList::new();
        let view = LagometerView {
            frame: vec![5_000; 10],
            sync: vec![0; 10],
            upload: vec![0; 10],
        };
        draw_lagometer(&mut list, &view, 200.0, 1.0, 4);
        // The backdrop plus four frame bars; zero timers draw nothing.
        assert_eq!(rects(&list).len(), 5);
    }

    #[test]
    fn a_slow_frame_is_red_and_the_bar_stops_at_the_graph_top() {
        let mut list = DrawList::new();
        let view = LagometerView {
            frame: vec![1_000_000],
            ..LagometerView::default()
        };
        draw_lagometer(&mut list, &view, 200.0, 1.0, 8);
        let bars = rects(&list);
        assert_eq!(bars[1].height, GRAPH_HEIGHT);
        let colours: Vec<Color> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Rect { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        assert_eq!(colours[1], FRAME_SLOW);
    }

    #[test]
    fn an_empty_lagometer_draws_nothing() {
        let mut list = DrawList::new();
        draw_lagometer(&mut list, &LagometerView::default(), 200.0, 1.0, 8);
        assert!(rects(&list).is_empty());
    }

    #[test]
    fn the_fps_line_sits_on_the_first_row() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        draw_fps_line(&mut list, "60 fps", 1.0, &font);
        assert_eq!(rects(&list)[0].y, 1.0);
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
