// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat on the HUD: the log with its colours, the fade, the scrolling and
//! the input line (M10, GAM-6).
//!
//! The rules follow MCP `GuiNewChat.drawChat` (a line is visible for 200
//! ticks, then fades; an open chat shows every line at full opacity; lines
//! are wrapped and scrolled from the newest) and `GuiChat` (the input bar at
//! the bottom of the screen). Like the rest of the HUD it draws with the
//! built-in font: bold is drawn twice, underline and strikethrough are
//! rectangles, obfuscated text is replaced by changing glyphs, and italic has
//! no effect because the font cannot be sheared.

use std::time::Instant;

use crate::font;
use crate::hud::{ChatInputView, ChatLineView, ChatRunView, HudLayout, HudState, CHAT_LINES};
use crate::overlay::Overlay;

/// The chat shows this many rows while it is open.
const OPEN_LINES: usize = 20;
/// A line stays fully visible for this many ticks before it fades.
const VISIBLE_TICKS: f32 = 200.0;
/// The milliseconds of one game tick.
const TICK_MS: u128 = 50;
/// Indent of the text inside its background, in pixels.
const TEXT_INDENT: f32 = 2.0;
/// Left edge of the chat block, in pixels.
const LEFT: f32 = 4.0;
/// The opacity below which a line is not drawn (`l1 > 3` out of 255).
const MIN_ALPHA: f32 = 3.0 / 255.0;
/// Background of one chat row, at full opacity (`l1 / 2 << 24`).
const ROW_BACKGROUND: f32 = 0.5;
/// Background of the input bar (`Integer.MIN_VALUE`).
const INPUT_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
/// The glyphs an obfuscated character cycles through.
const SCRAMBLE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789";

/// The opacity of a chat line `age_ticks` old: opaque while the chat is open,
/// otherwise `clamp((1 - age / 200) * 10, 0, 1)` squared.
#[must_use]
pub fn line_alpha(age_ticks: f32, open: bool) -> f32 {
    if open {
        return 1.0;
    }
    let fade = ((1.0 - age_ticks / VISIBLE_TICKS) * 10.0).clamp(0.0, 1.0);
    fade * fade
}

/// The game ticks between `received` and `now`.
fn age_ticks(received: Instant, now: Option<Instant>) -> f32 {
    now.map_or(0.0, |now| {
        (now.saturating_duration_since(received).as_millis() / TICK_MS) as f32
    })
}

/// Breaks a line into rows of at most `max_chars` characters, at the last
/// space that fits (`listFormattedStringToWidth`), or in the middle of a word
/// that is longer than a row. Each row keeps the style of its characters.
#[must_use]
pub fn wrap(line: &ChatLineView, max_chars: usize) -> Vec<Vec<ChatRunView>> {
    let max = max_chars.max(1);
    let chars: Vec<(char, usize)> = line
        .runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| run.text.chars().map(move |character| (character, index)))
        .collect();

    let mut rows = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let (end, next) = if chars.len() - start <= max {
            (chars.len(), chars.len())
        } else {
            let window_end = start + max;
            match (start + 1..=window_end)
                .rev()
                .find(|&index| chars[index].0 == ' ')
            {
                Some(space) => (space, space + 1),
                None => (window_end, window_end),
            }
        };
        rows.push(regroup(&line.runs, &chars[start..end]));
        start = next;
    }
    if rows.is_empty() {
        rows.push(Vec::new());
    }
    rows
}

/// The characters of one row grouped back into runs of one style.
fn regroup(runs: &[ChatRunView], chars: &[(char, usize)]) -> Vec<ChatRunView> {
    let mut row: Vec<ChatRunView> = Vec::new();
    let mut current: Option<usize> = None;
    for &(character, run) in chars {
        if current != Some(run) {
            current = Some(run);
            row.push(ChatRunView {
                text: String::new(),
                ..runs[run].clone()
            });
        }
        if let Some(piece) = row.last_mut() {
            piece.text.push(character);
        }
    }
    row
}

/// The number of characters that fit in `width` pixels.
fn max_chars(width: f32, advance: f32) -> usize {
    if advance <= 0.0 {
        return 0;
    }
    (width / advance).floor().max(0.0) as usize
}

/// Draws the chat log and, when the chat is open, the input line.
pub(crate) fn draw(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    draw_log(overlay, state, layout);
    if let Some(input) = &state.chat_input {
        draw_input(overlay, input, layout);
    }
}

/// A row of the log with the opacity of the line it came from.
struct Row {
    pieces: Vec<ChatRunView>,
    alpha: f32,
}

fn draw_log(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    if state.chat.is_empty() {
        return;
    }
    let scale = layout.scale();
    let advance = font::CELL_WIDTH as f32 * scale;
    let open = state.chat_open;
    let row_chars = max_chars(layout.width() - LEFT - 2.0 * TEXT_INDENT, advance);

    let mut rows: Vec<Row> = Vec::new();
    for line in &state.chat {
        let alpha = line_alpha(age_ticks(line.received, state.now), open);
        for pieces in wrap(line, row_chars) {
            rows.push(Row { pieces, alpha });
        }
    }

    let bottom = layout.content_top(state.armor > 0.0);
    let line_height = layout.line_height();
    let fit = (bottom / line_height).floor().max(0.0) as usize;
    let limit = if open { OPEN_LINES } else { CHAT_LINES }.min(fit);
    if limit == 0 {
        return;
    }
    let scroll = if open {
        state.chat_scroll.min(rows.len().saturating_sub(limit))
    } else {
        0
    };
    let end = rows.len() - scroll;
    let first = end.saturating_sub(limit);
    let shown = &rows[first..end];
    let seed = state
        .now
        .map_or(0, |now| now.elapsed().as_millis() as usize / 50);
    for (index, row) in shown.iter().enumerate() {
        if row.alpha <= MIN_ALPHA {
            continue;
        }
        let band = layout.chat_line_rect(bottom, index, shown.len());
        let width: usize = row.pieces.iter().map(|p| p.text.chars().count()).sum();
        overlay.rect(
            band.x,
            band.y,
            width as f32 * advance + 2.0 * TEXT_INDENT,
            band.height,
            [0.0, 0.0, 0.0, ROW_BACKGROUND * row.alpha],
        );
        draw_pieces(
            overlay,
            band.x + TEXT_INDENT,
            band.y + (band.height - font::CELL_HEIGHT as f32 * scale) * 0.5,
            scale,
            &row.pieces,
            row.alpha,
            seed.wrapping_add(first + index),
        );
    }

    if open && rows.len() > limit {
        draw_scrollbar(overlay, layout, bottom, limit, rows.len(), scroll);
    }
}

/// Draws the runs of one row from `x`, returning where the text ends.
fn draw_pieces(
    overlay: &mut Overlay,
    x: f32,
    y: f32,
    scale: f32,
    pieces: &[ChatRunView],
    alpha: f32,
    seed: usize,
) -> f32 {
    let advance = font::CELL_WIDTH as f32 * scale;
    let glyph_height = font::CELL_HEIGHT as f32 * scale;
    let mut cursor = x;
    for piece in pieces {
        let width = piece.text.chars().count() as f32 * advance;
        let [red, green, blue] = piece.color;
        let color = [
            f32::from(red) / 255.0,
            f32::from(green) / 255.0,
            f32::from(blue) / 255.0,
            alpha,
        ];
        let shown = if piece.obfuscated {
            scramble(&piece.text, seed)
        } else {
            piece.text.clone()
        };
        overlay.text(cursor, y, scale, color, &shown);
        if piece.bold {
            overlay.text(cursor + scale, y, scale, color, &shown);
        }
        if piece.underlined {
            overlay.rect(cursor, y + glyph_height + scale * 0.5, width, scale, color);
        }
        if piece.strikethrough {
            overlay.rect(cursor, y + glyph_height * 0.5, width, scale, color);
        }
        cursor += width;
    }
    cursor
}

/// Replaces every visible character by a glyph that changes with `seed`, so
/// obfuscated text keeps its width but cannot be read.
fn scramble(text: &str, seed: usize) -> String {
    text.chars()
        .enumerate()
        .map(|(index, character)| {
            if character == ' ' {
                ' '
            } else {
                let pick = (index.wrapping_mul(31).wrapping_add(seed)) % SCRAMBLE.len();
                SCRAMBLE[pick] as char
            }
        })
        .collect()
}

/// The scroll position of an open chat, from the bottom.
fn draw_scrollbar(
    overlay: &mut Overlay,
    layout: &HudLayout,
    bottom: f32,
    limit: usize,
    total: usize,
    scroll: usize,
) {
    let scale = layout.scale();
    let track = limit as f32 * layout.line_height();
    let thumb = (track * limit as f32 / total as f32).max(2.0 * scale);
    let travel = (track - thumb).max(0.0);
    let hidden = (total - limit).max(1) as f32;
    let top = bottom - thumb - travel * scroll as f32 / hidden;
    let color = if scroll > 0 {
        [0.8, 0.8, 0.8, 0.67]
    } else {
        [0.8, 0.8, 0.8, 0.38]
    };
    overlay.rect(
        (LEFT - 3.0 * scale).max(0.0),
        top,
        2.0 * scale,
        thumb,
        color,
    );
}

/// Draws the input bar at the bottom of the screen: the text, scrolled so the
/// caret stays visible, and the caret.
fn draw_input(overlay: &mut Overlay, input: &ChatInputView, layout: &HudLayout) {
    let scale = layout.scale();
    let advance = font::CELL_WIDTH as f32 * scale;
    let bar_height = layout.line_height() + 2.0 * scale;
    let y = layout.height() - bar_height - 2.0 * scale;
    overlay.rect(
        2.0,
        y,
        (layout.width() - 4.0).max(0.0),
        bar_height,
        INPUT_BACKGROUND,
    );

    let room = max_chars(layout.width() - 8.0 - advance, advance).max(1);
    let chars: Vec<char> = input.text.chars().collect();
    let caret = input.caret.min(chars.len());
    let start = (caret + 1).saturating_sub(room);
    let shown: String = chars[start..].iter().take(room).collect();
    let text_y = y + (bar_height - font::CELL_HEIGHT as f32 * scale) * 0.5;
    overlay.text(4.0, text_y, scale, [1.0, 1.0, 1.0, 1.0], &shown);

    if input.caret_visible {
        let caret_x = 4.0 + (caret - start) as f32 * advance;
        if caret == chars.len() {
            overlay.text(caret_x, text_y, scale, [1.0, 1.0, 1.0, 1.0], "_");
        } else {
            overlay.rect(
                caret_x,
                text_y - scale,
                scale.max(1.0),
                font::CELL_HEIGHT as f32 * scale + 2.0 * scale,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn run(text: &str) -> ChatRunView {
        ChatRunView {
            text: text.to_string(),
            color: [255, 255, 255],
            bold: false,
            italic: false,
            underlined: false,
            strikethrough: false,
            obfuscated: false,
        }
    }

    fn line(runs: Vec<ChatRunView>, received: Instant) -> ChatLineView {
        ChatLineView { runs, received }
    }

    fn texts(rows: &[Vec<ChatRunView>]) -> Vec<String> {
        rows.iter()
            .map(|row| row.iter().map(|piece| piece.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn a_line_fades_after_two_hundred_ticks_and_stays_opaque_when_open() {
        assert_eq!(line_alpha(0.0, false), 1.0);
        assert_eq!(line_alpha(180.0, false), 1.0);
        let fading = line_alpha(190.0, false);
        assert!(fading > 0.0 && fading < 1.0, "fading was {fading}");
        assert_eq!(line_alpha(200.0, false), 0.0);
        assert_eq!(line_alpha(10_000.0, false), 0.0);
        assert_eq!(line_alpha(10_000.0, true), 1.0);
    }

    #[test]
    fn the_fade_is_quadratic() {
        // 195 ticks: (1 - 0.975) * 10 = 0.25, squared 0.0625.
        assert!((line_alpha(195.0, false) - 0.0625).abs() < 1e-4);
    }

    #[test]
    fn short_lines_are_one_row() {
        let rows = wrap(&line(vec![run("hello")], Instant::now()), 20);
        assert_eq!(texts(&rows), ["hello"]);
    }

    #[test]
    fn long_lines_wrap_at_the_last_space_that_fits() {
        let rows = wrap(&line(vec![run("aaa bbb ccc ddd")], Instant::now()), 8);
        assert_eq!(texts(&rows), ["aaa bbb", "ccc ddd"]);
    }

    #[test]
    fn a_word_longer_than_a_row_is_cut_in_the_middle() {
        let rows = wrap(&line(vec![run("abcdefghij")], Instant::now()), 4);
        assert_eq!(texts(&rows), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn a_row_may_be_exactly_as_long_as_the_limit_when_a_space_follows() {
        let rows = wrap(&line(vec![run("abcd efgh")], Instant::now()), 4);
        assert_eq!(texts(&rows), ["abcd", "efgh"]);
    }

    #[test]
    fn rows_keep_the_style_of_each_run_across_a_wrap() {
        let mut red = run("bbbb");
        red.color = [255, 85, 85];
        let rows = wrap(&line(vec![run("aaaa "), red], Instant::now()), 6);
        assert_eq!(texts(&rows), ["aaaa", "bbbb"]);
        assert_eq!(rows[1][0].color, [255, 85, 85]);
    }

    #[test]
    fn an_empty_line_still_takes_a_row() {
        let rows = wrap(&line(vec![], Instant::now()), 10);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_empty());
        // A zero width cannot loop forever either.
        assert!(!wrap(&line(vec![run("ab")], Instant::now()), 0).is_empty());
    }

    #[test]
    fn scrambled_text_keeps_its_width_and_hides_the_text() {
        let scrambled = scramble("hello world", 3);
        assert_eq!(scrambled.chars().count(), 11);
        assert_eq!(scrambled.chars().nth(5), Some(' '));
        assert_ne!(scrambled, "hello world");
        assert_ne!(scramble("abc", 0), scramble("abc", 1));
    }

    fn state_with(lines: Vec<ChatLineView>) -> HudState {
        HudState {
            chat: lines,
            ..HudState::default()
        }
    }

    fn drawn(state: &HudState) -> u32 {
        let mut overlay = Overlay::new(1280, 720);
        draw(&mut overlay, state, &HudLayout::new(1280, 720));
        overlay.vertex_count()
    }

    #[test]
    fn old_lines_disappear_unless_the_chat_is_open() {
        let now = Instant::now();
        let old = now - Duration::from_secs(60);
        let mut state = state_with(vec![line(vec![run("OLD")], old)]);
        state.now = Some(now);
        assert_eq!(drawn(&state), 0, "a minute-old line has faded away");
        state.chat_open = true;
        assert!(drawn(&state) > 0, "an open chat shows everything");
    }

    #[test]
    fn a_fresh_line_is_drawn() {
        let now = Instant::now();
        let mut state = state_with(vec![line(vec![run("HELLO")], now)]);
        state.now = Some(now);
        assert!(drawn(&state) > 0);
    }

    #[test]
    fn bold_underline_and_strikethrough_add_geometry() {
        let now = Instant::now();
        let plain = state_with(vec![line(vec![run("TEXT")], now)]);
        let mut styled_run = run("TEXT");
        styled_run.bold = true;
        styled_run.underlined = true;
        styled_run.strikethrough = true;
        let styled = state_with(vec![line(vec![styled_run], now)]);
        assert!(drawn(&styled) > drawn(&plain));
    }

    #[test]
    fn the_input_bar_and_the_caret_are_drawn() {
        let mut state = HudState {
            chat_input: Some(ChatInputView {
                text: "HELLO".to_string(),
                caret: 5,
                caret_visible: false,
            }),
            ..HudState::default()
        };
        let without_caret = drawn(&state);
        assert!(without_caret > 0);
        state.chat_input = Some(ChatInputView {
            text: "HELLO".to_string(),
            caret: 5,
            caret_visible: true,
        });
        assert!(drawn(&state) > without_caret);
    }

    #[test]
    fn a_very_long_input_and_a_hundred_lines_stay_on_screen() {
        let now = Instant::now();
        let mut state = state_with(
            (0..100)
                .map(|i| {
                    line(
                        vec![run(&format!("MESSAGE NUMBER {i} {}", "X".repeat(150)))],
                        now,
                    )
                })
                .collect(),
        );
        state.now = Some(now);
        state.chat_open = true;
        state.chat_scroll = 30;
        state.chat_input = Some(ChatInputView {
            text: "Y".repeat(100),
            caret: 50,
            caret_visible: true,
        });
        let mut overlay = Overlay::new(1280, 720);
        draw(&mut overlay, &state, &HudLayout::new(1280, 720));
        assert!(overlay.vertex_count() > 0);
        for vertex in overlay.vertices() {
            assert!(vertex.position[0].abs() <= 1.001, "x {:?}", vertex.position);
            assert!(vertex.position[1].abs() <= 1.001, "y {:?}", vertex.position);
        }
    }

    #[test]
    fn scrolling_past_the_start_is_clamped() {
        let now = Instant::now();
        let mut state = state_with(vec![line(vec![run("ONLY")], now)]);
        state.now = Some(now);
        state.chat_open = true;
        state.chat_scroll = 10_000;
        assert!(drawn(&state) > 0);
    }
}
