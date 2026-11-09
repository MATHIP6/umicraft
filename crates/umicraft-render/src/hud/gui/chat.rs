// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat: the wrapped, fading log, its scrollbar and the input line.
//!
//! Rules follow MCP `GuiNewChat.drawChat`, `GuiNewChat.getChatComponent` and
//! `GuiChat`. The rows are wrapped with the proportional font by
//! [`umicraft_game::chat::wrap`], the text is drawn with
//! [`umicraft_ui::styled_text::draw_styled`].

use umicraft_game::chat::wrap::{self, ChatGeometry, Piece};
use umicraft_ui::screens::gui_scale;
use umicraft_ui::styled_text::{draw_styled, StyledRun};
use umicraft_ui::{Color, DrawList, FontAtlas, Rect, TextStyle};

use super::fade::{age_ticks, line_alpha};
use crate::hud::{ChatRunView, HudState};

/// The wrapped rows `GuiNewChat` keeps (`field_146253_i` is cut at 100).
const MAX_ROWS: usize = 100;

impl Piece for ChatRunView {
    fn text(&self) -> &str {
        &self.text
    }

    fn bold(&self) -> bool {
        self.bold
    }

    fn with_text(&self, text: String) -> Self {
        Self {
            text,
            ..self.clone()
        }
    }
}

/// A wrapped row of chat text with its computed opacity.
struct ChatRow {
    pieces: Vec<ChatRunView>,
    alpha: f32,
}

/// The wrapped rows of the log, oldest first, at most [`MAX_ROWS`].
///
/// `GuiNewChat.setChatLine` wraps every message to `floor(chatWidth /
/// chatScale)` with the proportional font and drops the oldest rows beyond
/// 100. With `wanted` the newest messages are wrapped until that many rows are
/// there and the older ones are left alone: a closed chat shows ten rows, and
/// wrapping the whole history for them cost 160 us a frame (perf 2, B1). The
/// rows returned are the newest ones, so the slice a window shows is the same.
fn chat_rows(state: &HudState, font: &FontAtlas, wanted: Option<usize>) -> Vec<ChatRow> {
    let opacity = state.chat_style.opacity * 0.9 + 0.1;
    let max_width = (state.chat_style.width_px() as f32 / state.chat_style.scale).floor();
    let advance = |character: char| font.advance(character, 1.0);
    let wanted = wanted.map_or(usize::MAX, |wanted| wanted.min(MAX_ROWS));
    // Newest first, so the walk can stop; reversed at the end.
    let mut rows: Vec<ChatRow> = Vec::new();
    for line in state.chat.iter().rev() {
        if rows.len() >= wanted {
            break;
        }
        let age = age_ticks(line.received, state.now);
        let alpha = line_alpha(age, state.chat_open) * opacity;
        let wrapped = wrap::wrap(&line.runs, max_width, &advance);
        rows.extend(
            wrapped
                .into_iter()
                .rev()
                .map(|pieces| ChatRow { pieces, alpha }),
        );
    }
    rows.reverse();
    rows.drain(..rows.len().saturating_sub(MAX_ROWS));
    rows
}

/// How many rows the window of `state` can show when it is closed: the rows the
/// closed chat needs, and no more.
fn rows_needed(state: &HudState) -> Option<usize> {
    if state.chat_open {
        return None;
    }
    Some((state.chat_style.height_unfocused_px() / 9).max(0) as usize)
}

/// The slice of rows the chat shows (`first..end`) and how far it is
/// scrolled back, for `total` wrapped rows.
fn visible_window(state: &HudState, total: usize) -> (usize, usize, usize) {
    let style = state.chat_style;
    let height = if state.chat_open {
        style.height_focused_px()
    } else {
        style.height_unfocused_px()
    };
    let limit = (height / 9).max(0) as usize;
    let scroll = if state.chat_open {
        state.chat_scroll.min(total.saturating_sub(limit))
    } else {
        0
    };
    let end = total - scroll;
    (end.saturating_sub(limit), end, scroll)
}

/// The run of the open chat under `pointer` (`GuiNewChat.getChatComponent`).
///
/// `pointer` is in physical pixels from the top-left of a `viewport` window.
/// `None` when the chat is closed or the pointer is over no text.
#[must_use]
pub fn chat_run_at(
    state: &HudState,
    viewport: (u32, u32),
    font: &FontAtlas,
    pointer: (f32, f32),
) -> Option<ChatRunView> {
    if !state.chat_open {
        return None;
    }
    let rows = chat_rows(state, font, None);
    let (_, _, scroll) = visible_window(state, rows.len());
    let style = state.chat_style;
    let geometry = ChatGeometry {
        scale_factor: i32::try_from(gui_scale(viewport.0, viewport.1)).unwrap_or(1),
        chat_scale: style.scale,
        width: style.width_px(),
        line_count: (style.height_focused_px() / 9).max(0) as usize,
    };
    // LWJGL reports the pointer from the bottom-left.
    let from_bottom = (pointer.0 as i32, viewport.1 as i32 - pointer.1 as i32);
    let hit = wrap::hit_row(from_bottom, &geometry, rows.len(), scroll)?;
    let row = rows.get(rows.len() - 1 - hit.row)?;
    let index = wrap::piece_at(&row.pieces, hit.x, &|c| font.advance(c, 1.0))?;
    row.pieces.get(index).cloned()
}

/// The run `run` of a chat row as a styled run with the row's opacity.
fn styled(run: &ChatRunView, alpha: f32) -> StyledRun {
    let channel = |value: u8| f32::from(value) / 255.0;
    StyledRun {
        text: run.text.clone(),
        color: [
            channel(run.color[0]),
            channel(run.color[1]),
            channel(run.color[2]),
            alpha,
        ],
        bold: run.bold,
        italic: run.italic,
        underlined: run.underlined,
        strikethrough: run.strikethrough,
        obfuscated: run.obfuscated,
    }
}

/// The log of `GuiNewChat.drawChat`: everything is in the units of a frame
/// scaled by `chatScale`, from the origin (2, h - 28).
fn draw_chat_log(list: &mut DrawList, state: &HudState, gh: f32, sf: f32, font: &FontAtlas) {
    let open = state.chat_open;
    let style = state.chat_style;
    if style.scale <= 0.0 {
        return;
    }
    let k = sf * style.scale;
    let band = style.width_px() as f32 / style.scale;
    // The band is `l + 4` wide (the text itself wraps at the floor of `l`).
    let chat_width = band.ceil() + 4.0;

    let rows = chat_rows(state, font, rows_needed(state));
    let (first, end, scroll) = visible_window(state, rows.len());
    let visible = &rows[first..end];

    // Obfuscated glyphs change every 50 ms.
    let tick = state
        .now
        .map_or(0, |now| (now.elapsed().as_millis() / 50) as u32);

    let (ox, oy) = (2.0 * sf, (gh - 28.0) * sf);
    for (i, row) in visible.iter().rev().enumerate() {
        if row.alpha <= (3.0 / 255.0) {
            continue;
        }
        let local_y = -(i as f32 + 1.0) * 9.0;
        let band_rect = Rect::new(ox, oy + local_y * k, chat_width * k, 9.0 * k);
        list.rect(band_rect, [0.0, 0.0, 0.0, row.alpha * 0.5]);

        let text_y = oy + (local_y + 1.0) * k;
        let runs: Vec<StyledRun> = row
            .pieces
            .iter()
            .map(|run| styled(run, row.alpha))
            .collect();
        draw_styled(list, font, [ox, text_y], &runs, k, true, tick);
    }

    if open {
        draw_chat_scrollbar(list, rows.len(), visible.len(), scroll, (ox, oy, k));
    }
}

/// Draws the chat log and, while open, the input line.
pub(super) fn draw_chat(
    list: &mut DrawList,
    state: &HudState,
    gw: f32,
    gh: f32,
    sf: f32,
    font: &FontAtlas,
) {
    draw_chat_log(list, state, gh, sf, font);

    // Chat input bar
    if let Some(input) = &state.chat_input {
        let input_rect = Rect::new(2.0 * sf, (gh - 14.0) * sf, (gw - 4.0) * sf, 12.0 * sf);
        list.rect(input_rect, [0.0, 0.0, 0.0, 0.5]);

        // `GuiTextField`: 0xE0E0E0 with a shadow, at (4, h - 12).
        let text = &input.text;
        let ink: Color = [0.878, 0.878, 0.878, 1.0];
        list.text(
            [(4.0 + 1.0) * sf, (gh - 12.0 + 1.0) * sf],
            text,
            TextStyle::new(sf, [0.22, 0.22, 0.22, 1.0]),
        );
        list.text([4.0 * sf, (gh - 12.0) * sf], text, TextStyle::new(sf, ink));

        if input.caret_visible {
            // The caret is a character index, not a byte offset.
            let before: String = text.chars().take(input.caret).collect();
            let caret_x = 4.0 + font.measure(&before, 1.0);
            if input.caret >= text.chars().count() {
                // At the end vanilla draws an underscore, not a bar.
                list.text(
                    [(caret_x + 1.0) * sf, (gh - 12.0 + 1.0) * sf],
                    "_",
                    TextStyle::new(sf, [0.22, 0.22, 0.22, 1.0]),
                );
                list.text(
                    [caret_x * sf, (gh - 12.0) * sf],
                    "_",
                    TextStyle::new(sf, ink),
                );
            } else {
                // `-3092272` = 0xFFD0D0D0, one unit wide, ten tall.
                list.rect(
                    Rect::new(caret_x * sf, (gh - 13.0) * sf, sf, 10.0 * sf),
                    [0.816, 0.816, 0.816, 1.0],
                );
            }
        }
    }
}

/// The scrollbar `GuiNewChat.drawChat` draws at the left of an open chat.
fn draw_chat_scrollbar(
    list: &mut DrawList,
    total: usize,
    shown: usize,
    scroll: usize,
    (ox, oy, k): (f32, f32, f32),
) {
    if total == 0 {
        return;
    }
    let (t, j) = (total as f32, shown as f32);
    let full = t * 10.0;
    let window = j * 10.0;
    if (full - window).abs() < f32::EPSILON {
        return;
    }
    let offset = (scroll as f32 * window / t).floor();
    let thumb = (window * window / full).floor();
    let alpha = if offset > 0.0 { 170.0 } else { 96.0 } / 255.0;
    let (top, height) = (oy - (offset + thumb) * k, thumb * k);
    // 0xCC3333 while scrolled back, 0x33332A at the bottom.
    let fill: Color = if scroll > 0 {
        [0.8, 0.2, 0.2, alpha]
    } else {
        [0.2, 0.2, 0.165, alpha]
    };
    list.rect(Rect::new(ox - 3.0 * k, top, 2.0 * k, height), fill);
    list.rect(Rect::new(ox - k, top, k, height), [0.8, 0.8, 0.8, alpha]);
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::hud::{ChatInputView, ChatLineView};

    fn run(text: &str) -> ChatRunView {
        ChatRunView {
            text: text.to_string(),
            color: [255; 3],
            ..ChatRunView::default()
        }
    }

    fn state_with(lines: Vec<ChatLineView>) -> HudState {
        HudState {
            chat: lines,
            chat_open: true,
            now: Some(Instant::now()),
            ..HudState::default()
        }
    }

    fn line(text: &str) -> ChatLineView {
        ChatLineView {
            runs: vec![run(text)],
            received: Instant::now(),
        }
    }

    #[test]
    fn long_lines_wrap_into_several_rows_and_an_empty_one_is_blank() {
        let font = FontAtlas::fallback();
        let long = "word ".repeat(80);
        let state = state_with(vec![line(&long), line("")]);
        let rows = chat_rows(&state, &font, None);
        assert!(rows.len() > 2);
        assert!(rows.last().is_some_and(|row| row.pieces.is_empty()));
    }

    #[test]
    fn the_log_keeps_only_the_newest_hundred_wrapped_rows() {
        let font = FontAtlas::fallback();
        let lines = (0..150).map(|n| line(&format!("message {n}"))).collect();
        let rows = chat_rows(&state_with(lines), &font, None);
        assert_eq!(rows.len(), MAX_ROWS);
        assert_eq!(rows[MAX_ROWS - 1].pieces[0].text, "message 149");
    }

    #[test]
    fn the_pointer_finds_the_run_under_it_with_its_events() {
        let font = FontAtlas::fallback();
        let mut linked = run("click");
        linked.insertion = Some("Steve".to_string());
        let mut state = state_with(vec![ChatLineView {
            runs: vec![run("ab "), linked],
            received: Instant::now(),
        }]);
        state.chat_style.scale = 1.0;
        let viewport = (640, 360);
        // The gui scale is 1 at this size; the newest row is hit from 27 to 36
        // units above the window bottom, so y = 330 from the top.
        let hit = chat_run_at(&state, viewport, &font, (23.0, 330.0));
        assert_eq!(hit.and_then(|run| run.insertion), Some("Steve".to_string()));
        let on_first = chat_run_at(&state, viewport, &font, (6.0, 330.0));
        assert_eq!(on_first.map(|run| run.text), Some("ab ".to_string()));
        assert!(chat_run_at(&state, viewport, &font, (600.0, 330.0)).is_none());
        state.chat_open = false;
        assert!(chat_run_at(&state, viewport, &font, (6.0, 330.0)).is_none());
    }

    #[test]
    fn a_non_ascii_caret_position_does_not_split_a_character() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let state = HudState {
            chat_input: Some(ChatInputView {
                text: "\u{e9}\u{e9}x".to_string(),
                caret: 1,
                caret_visible: true,
            }),
            ..HudState::default()
        };
        // A byte slice at the character index 1 would panic inside "\u{e9}".
        draw_chat(&mut list, &state, 320.0, 180.0, 2.0, &font);
        assert!(!list.is_empty());
    }

    #[test]
    fn a_closed_chat_wraps_only_the_newest_messages_and_shows_the_same_rows() {
        let font = FontAtlas::fallback();
        let lines: Vec<ChatLineView> = (0..60)
            .map(|n| line(&format!("message number {n}")))
            .collect();
        let state = HudState {
            chat_open: false,
            ..state_with(lines)
        };
        let everything = chat_rows(&state, &font, None);
        let tail = chat_rows(&state, &font, rows_needed(&state));
        assert!(
            tail.len() < everything.len(),
            "the old history is left alone"
        );
        let (first, end, _) = visible_window(&state, tail.len());
        let (all_first, all_end, _) = visible_window(&state, everything.len());
        let shown = |rows: &[ChatRow], from: usize, to: usize| -> Vec<String> {
            rows[from..to]
                .iter()
                .map(|row| {
                    row.pieces
                        .iter()
                        .map(|piece| piece.text.as_str())
                        .collect::<String>()
                })
                .collect()
        };
        assert_eq!(
            shown(&tail, first, end),
            shown(&everything, all_first, all_end)
        );
    }
}
