// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat: the wrapped, fading log, its scrollbar and the input line.
//!
//! Rules follow MCP `GuiNewChat.drawChat` and `GuiChat`.

use umicraft_ui::{Color, DrawList, FontAtlas, Rect, TextStyle};

use crate::hud::chat_hud::{self, line_alpha};
use crate::hud::{ChatLineView, ChatRunView, HudState};

/// The glyphs an obfuscated character cycles through.
const SCRAMBLE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789";

/// A wrapped row of chat text with its computed opacity.
struct ChatRow {
    pieces: Vec<ChatRunView>,
    alpha: f32,
}

/// Wraps a chat line using proportional font glyph metrics (`font.advance`).
fn wrap_proportional(
    line: &ChatLineView,
    max_width: f32,
    font: &FontAtlas,
) -> Vec<Vec<ChatRunView>> {
    let chars: Vec<(char, usize, f32)> = line
        .runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| {
            run.text.chars().map(move |character| {
                let adv = font.advance(character, 1.0) + if run.bold { 1.0 } else { 0.0 };
                (character, index, adv)
            })
        })
        .collect();

    let mut rows = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut width = 0.0;
        let mut end = start;
        while end < chars.len() && width + chars[end].2 <= max_width {
            width += chars[end].2;
            end += 1;
        }

        let (chosen_end, next) = if end >= chars.len() {
            (chars.len(), chars.len())
        } else {
            // Find last space
            match (start + 1..=end).rev().find(|&i| chars[i - 1].0 == ' ') {
                Some(space) => (space - 1, space),
                None => (end.max(start + 1), end.max(start + 1)),
            }
        };

        let raw_chars: Vec<(char, usize)> = chars[start..chosen_end]
            .iter()
            .map(|&(c, idx, _)| (c, idx))
            .collect();
        rows.push(chat_hud::regroup(&line.runs, &raw_chars));
        start = next;
    }

    if rows.is_empty() {
        rows.push(Vec::new());
    }
    rows
}

/// The log of `GuiNewChat.drawChat`: everything is in the units of a frame
/// scaled by `chatScale`, from the origin (2, h - 28).
fn draw_chat_log(list: &mut DrawList, state: &HudState, gh: f32, sf: f32, font: &FontAtlas) {
    let open = state.chat_open;
    let style = state.chat_style;
    if style.scale <= 0.0 {
        return;
    }
    let opacity = style.opacity * 0.9 + 0.1;
    let k = sf * style.scale;
    let band = style.width_px() as f32 / style.scale;
    // The text wraps at the floor of the width and the band is `l + 4` wide.
    let text_max_w = band.floor();
    let chat_width = band.ceil() + 4.0;

    let mut rows: Vec<ChatRow> = Vec::new();
    for line in &state.chat {
        let age = chat_hud::age_ticks(line.received, state.now);
        let alpha = line_alpha(age, open) * opacity;
        for pieces in wrap_proportional(line, text_max_w, font) {
            rows.push(ChatRow { pieces, alpha });
        }
    }

    let height = if open {
        style.height_focused_px()
    } else {
        style.height_unfocused_px()
    };
    let limit = (height / 9).max(0) as usize;
    let scroll = if open {
        state.chat_scroll.min(rows.len().saturating_sub(limit))
    } else {
        0
    };
    let end = rows.len().saturating_sub(scroll);
    let first = end.saturating_sub(limit);
    let visible = &rows[first..end];

    let seed = state
        .now
        .map_or(0, |now| now.elapsed().as_millis() as usize / 50);

    let (ox, oy) = (2.0 * sf, (gh - 28.0) * sf);
    for (i, row) in visible.iter().rev().enumerate() {
        if row.alpha <= (3.0 / 255.0) {
            continue;
        }
        let local_y = -(i as f32 + 1.0) * 9.0;
        let band_rect = Rect::new(ox, oy + local_y * k, chat_width * k, 9.0 * k);
        list.rect(band_rect, [0.0, 0.0, 0.0, row.alpha * 0.5]);

        let mut cur_x = 0.0;
        let text_y = oy + (local_y + 1.0) * k;
        for (run_idx, run) in row.pieces.iter().enumerate() {
            let run_text: String = if run.obfuscated {
                run.text
                    .chars()
                    .enumerate()
                    .map(|(char_idx, c)| {
                        if c == ' ' {
                            ' '
                        } else {
                            let idx = (seed + run_idx * 17 + char_idx * 31) % SCRAMBLE.len();
                            SCRAMBLE[idx] as char
                        }
                    })
                    .collect()
            } else {
                run.text.clone()
            };

            let text_color: Color = [
                run.color[0] as f32 / 255.0,
                run.color[1] as f32 / 255.0,
                run.color[2] as f32 / 255.0,
                row.alpha,
            ];
            let shadow_color: Color = [
                run.color[0] as f32 / 1020.0,
                run.color[1] as f32 / 1020.0,
                run.color[2] as f32 / 1020.0,
                row.alpha,
            ];

            let run_w = font.measure(&run_text, 1.0);
            let x = ox + cur_x * k;

            // Shadow
            list.text(
                [x + k, text_y + k],
                &run_text,
                TextStyle::new(k, shadow_color),
            );
            if run.bold {
                list.text(
                    [x + 2.0 * k, text_y + k],
                    &run_text,
                    TextStyle::new(k, shadow_color),
                );
            }

            // Main text
            list.text([x, text_y], &run_text, TextStyle::new(k, text_color));
            if run.bold {
                list.text([x + k, text_y], &run_text, TextStyle::new(k, text_color));
            }

            if run.underlined {
                list.rect(Rect::new(x, text_y + 8.0 * k, run_w * k, k), text_color);
            }
            if run.strikethrough {
                list.rect(Rect::new(x, text_y + 4.0 * k, run_w * k, k), text_color);
            }

            cur_x += run_w + if run.bold { 1.0 } else { 0.0 };
        }
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
            let caret_idx = input.caret.min(text.len());
            let caret_x = 4.0 + font.measure(&text[..caret_idx], 1.0);
            if caret_idx == text.len() {
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
    use super::*;

    #[test]
    fn wrapping_splits_long_lines_and_never_returns_nothing() {
        let font = FontAtlas::fallback();
        let line = ChatLineView {
            runs: vec![ChatRunView {
                text: "aaaa bbbb cccc".to_string(),
                color: [255; 3],
                bold: false,
                italic: false,
                underlined: false,
                strikethrough: false,
                obfuscated: false,
            }],
            received: std::time::Instant::now(),
        };
        assert!(wrap_proportional(&line, 20.0, &font).len() > 1);
        let empty = ChatLineView {
            runs: Vec::new(),
            ..line
        };
        assert_eq!(wrap_proportional(&empty, 20.0, &font).len(), 1);
    }
}
