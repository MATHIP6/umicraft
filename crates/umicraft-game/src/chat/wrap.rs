// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wrapping chat text to the chat width with a proportional font, and the
//! pointer hit-test of the chat log.
//!
//! Sources: MCP `GuiUtilRenderComponents.func_178908_a` (the wrapping loop,
//! called by `GuiNewChat.setChatLine` with `p_178908_3_ = false`, which keeps
//! the space a line was broken at at the start of the next line),
//! `FontRenderer.trimStringToWidth` and `GuiNewChat.getChatComponent` (the
//! pointer geometry).
//!
//! The code is generic over the piece type and the glyph advance so the HUD
//! (which owns the font) and the tests (which use a fixed advance) share it.

/// A styled piece of a chat line that wrapping may split.
pub trait Piece: Clone {
    /// The text of the piece.
    fn text(&self) -> &str;
    /// Whether the glyphs are bold (one unit wider each).
    fn bold(&self) -> bool;
    /// The same piece with other text.
    fn with_text(&self, text: String) -> Self;
}

impl Piece for super::component::Run {
    fn text(&self) -> &str {
        &self.text
    }

    fn bold(&self) -> bool {
        self.style.bold
    }

    fn with_text(&self, text: String) -> Self {
        Self::with_text(self, text)
    }
}

/// The width of `text` in GUI units (`FontRenderer.getStringWidth`).
fn width(advance: &dyn Fn(char) -> f32, text: &str, bold: bool) -> f32 {
    text.chars()
        .map(|character| advance(character) + if bold { 1.0 } else { 0.0 })
        .sum()
}

/// The longest prefix of `text` that fits in `limit`
/// (`FontRenderer.trimStringToWidth`), in characters.
fn fitting_prefix(advance: &dyn Fn(char) -> f32, text: &str, bold: bool, limit: f32) -> usize {
    let mut used = 0.0;
    let mut count = 0;
    for character in text.chars() {
        used += advance(character) + if bold { 1.0 } else { 0.0 };
        if used > limit {
            break;
        }
        count += 1;
    }
    count
}

/// The text of `text` split after `count` characters.
fn split_chars(text: &str, count: usize) -> (&str, &str) {
    let byte = text
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(at, _)| at);
    text.split_at(byte)
}

/// Wraps a chat line to `max_width` GUI units and returns its rows.
///
/// `advance` is the width of one glyph. A line always has at least one row, so
/// an empty message is a blank row. Rows hold no empty pieces.
#[must_use]
pub fn wrap<P: Piece>(pieces: &[P], max_width: f32, advance: &dyn Fn(char) -> f32) -> Vec<Vec<P>> {
    let mut queue: std::collections::VecDeque<P> = pieces.iter().cloned().collect();
    let mut rows = Vec::new();
    let mut row: Vec<P> = Vec::new();
    let mut used = 0.0;
    while let Some(piece) = queue.pop_front() {
        let bold = piece.bold();
        let mut text = piece.text().to_string();
        let mut broke = false;
        let mut forced = false;
        if let Some(newline) = text.find('\n') {
            queue.push_front(piece.with_text(text[newline + 1..].to_string()));
            text.truncate(newline + 1);
            broke = true;
        }
        let shown = text.strip_suffix('\n').unwrap_or(&text).to_string();
        let mut shown_width = width(advance, &shown, bold);
        let mut kept = shown;
        if used + shown_width > max_width {
            // Zero characters fit on an empty row: take one so a glyph wider
            // than the whole line cannot loop forever.
            let natural = fitting_prefix(advance, &text, bold, max_width - used);
            forced = natural == 0 && used == 0.0;
            let fit = natural.max(usize::from(forced));
            let (head, tail) = split_chars(&text, fit);
            let (mut head, mut tail) = (head.to_string(), tail.to_string());
            if !tail.is_empty() {
                match head.rfind(' ') {
                    Some(at) if width(advance, &text[..at], bold) > 0.0 => {
                        head = text[..at].to_string();
                        tail = text[at..].to_string();
                    }
                    _ if used > 0.0 && !text.contains(' ') => {
                        head.clear();
                        tail = text.clone();
                    }
                    _ => {}
                }
                queue.push_front(piece.with_text(tail));
            }
            shown_width = width(advance, &head, bold);
            kept = head;
            broke = true;
        }
        if forced || used + shown_width <= max_width {
            used += shown_width;
            if !kept.is_empty() {
                row.push(piece.with_text(kept));
            }
        } else {
            broke = true;
        }
        if broke {
            rows.push(std::mem::take(&mut row));
            used = 0.0;
        }
    }
    rows.push(row);
    rows
}

/// The geometry the pointer hit-test needs (`GuiNewChat`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatGeometry {
    /// The integer GUI scale factor (`ScaledResolution.getScaleFactor`).
    pub scale_factor: i32,
    /// `chatScale`.
    pub chat_scale: f32,
    /// `getChatWidth()`, in GUI units.
    pub width: i32,
    /// `getLineCount()`: how many rows the open chat shows.
    pub line_count: usize,
}

/// The row and the x offset under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatHit {
    /// The row, counted from the newest (`field_146253_i` index).
    pub row: usize,
    /// The pointer x inside the chat, in chat units.
    pub x: i32,
}

/// The row under the pointer (`GuiNewChat.getChatComponent`, first half).
///
/// `pointer` is in physical pixels with the origin at the bottom-left, as
/// LWJGL reports it. `rows` is the number of wrapped rows and `scroll` how far
/// the log is scrolled back. The reference leaves 3 units at the left and 27
/// at the bottom (the chat is drawn from 28), which is kept.
#[must_use]
pub fn hit_row(
    pointer: (i32, i32),
    geometry: &ChatGeometry,
    rows: usize,
    scroll: usize,
) -> Option<ChatHit> {
    let factor = geometry.scale_factor.max(1);
    let scale = geometry.chat_scale;
    if scale <= 0.0 {
        return None;
    }
    let x = ((pointer.0 / factor - 3) as f32 / scale).floor() as i32;
    let y = ((pointer.1 / factor - 27) as f32 / scale).floor() as i32;
    if x < 0 || y < 0 {
        return None;
    }
    let shown = i32::try_from(geometry.line_count.min(rows)).unwrap_or(i32::MAX);
    if x > (geometry.width as f32 / scale).floor() as i32 || y >= 9 * shown + shown {
        return None;
    }
    let row = usize::try_from(y / 9).ok()? + scroll;
    (row < rows).then_some(ChatHit { row, x })
}

/// The index of the piece at `x` in a row: the first one whose running width
/// passes `x` (`GuiNewChat.getChatComponent`, second half).
///
/// Like the reference, the width ignores bold.
#[must_use]
pub fn piece_at<P: Piece>(row: &[P], x: i32, advance: &dyn Fn(char) -> f32) -> Option<usize> {
    let mut end = 0.0;
    row.iter().position(|piece| {
        end += width(advance, piece.text(), false);
        end > x as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::component::Run;
    use crate::chat::style::Style;

    /// Every glyph is 6 units wide, like a proportional font's average.
    fn six(_: char) -> f32 {
        6.0
    }

    fn rows_text(rows: &[Vec<Run>]) -> Vec<String> {
        rows.iter()
            .map(|row| row.iter().map(|run| run.text.as_str()).collect())
            .collect()
    }

    fn wrapped(text: &str, max: f32) -> Vec<String> {
        rows_text(&wrap(&[Run::plain(text)], max, &six))
    }

    #[test]
    fn a_short_line_is_one_row_and_an_empty_one_a_blank_row() {
        assert_eq!(wrapped("hello", 60.0), ["hello"]);
        assert_eq!(wrapped("", 60.0), [""]);
    }

    #[test]
    fn a_long_line_breaks_at_the_last_space_that_fits_and_keeps_the_space() {
        // 12 glyphs fit in 72 units: "aaaa bbbb cc" fits, the break is the
        // last space before the limit.
        assert_eq!(
            wrapped("aaaa bbbb cccc dddd", 72.0),
            ["aaaa bbbb", " cccc dddd"],
            "the reference keeps the break space at the start of the next row"
        );
    }

    #[test]
    fn a_word_longer_than_the_row_is_cut_where_it_overflows() {
        assert_eq!(wrapped("abcdefghij", 36.0), ["abcdef", "ghij"]);
    }

    #[test]
    fn a_new_line_character_ends_the_row() {
        assert_eq!(wrapped("ab\ncd", 600.0), ["ab", "cd"]);
    }

    #[test]
    fn bold_pieces_are_wider() {
        let mut bold = Run::plain("abcd");
        bold.style = Style {
            bold: true,
            ..Style::default()
        };
        // 4 bold glyphs are 28 units: they do not fit in 27.
        assert_eq!(rows_text(&wrap(&[bold], 27.0, &six)).len(), 2);
    }

    #[test]
    fn styles_survive_the_split_and_a_piece_that_does_not_fit_moves_whole() {
        let mut red = Run::plain("xxxxxx");
        red.style.color = crate::chat::style::Color::from_name("red");
        let rows = wrap(&[Run::plain("abcd"), red.clone()], 36.0, &six);
        // "abcd" takes 24 units; the 6 red glyphs (36) do not fit the
        // remaining 12 and have no space, so they start the next row.
        assert_eq!(rows_text(&rows), ["abcd", "xxxxxx"]);
        assert_eq!(rows[1][0].style, red.style);
    }

    #[test]
    fn a_glyph_wider_than_the_row_cannot_loop_forever() {
        assert_eq!(wrapped("abc", 3.0).concat(), "abc");
    }

    #[test]
    fn no_row_exceeds_the_width_and_no_text_is_lost() {
        let text = "the quick brown fox jumps over the lazy dog and keeps running far away";
        let rows = wrapped(text, 90.0);
        assert!(rows
            .iter()
            .all(|row| row.chars().count() as f32 * 6.0 <= 90.0));
        assert_eq!(rows.concat(), text);
    }

    fn geometry() -> ChatGeometry {
        ChatGeometry {
            scale_factor: 2,
            chat_scale: 1.0,
            width: 320,
            line_count: 20,
        }
    }

    #[test]
    fn the_pointer_maps_to_a_row_counted_from_the_newest() {
        // y = 27 * 2 is the bottom edge of the newest row.
        let hit = hit_row((20, 54), &geometry(), 5, 0);
        assert_eq!(hit, Some(ChatHit { row: 0, x: 7 }));
        let second = hit_row((20, (27 + 9) * 2), &geometry(), 5, 0);
        assert_eq!(second.map(|hit| hit.row), Some(1));
    }

    #[test]
    fn scrolling_shifts_the_row_and_the_log_end_clips() {
        assert_eq!(
            hit_row((20, 54), &geometry(), 5, 2).map(|hit| hit.row),
            Some(2)
        );
        assert_eq!(hit_row((20, 54), &geometry(), 5, 5), None);
        assert_eq!(hit_row((20, (27 + 9 * 5) * 2), &geometry(), 5, 0), None);
    }

    #[test]
    fn outside_the_chat_nothing_is_hit() {
        assert_eq!(hit_row((4, 54), &geometry(), 5, 0), None, "left of x = 3");
        assert_eq!(hit_row((20, 40), &geometry(), 5, 0), None, "below y = 27");
        assert_eq!(
            hit_row((900, 54), &geometry(), 5, 0),
            None,
            "right of the width"
        );
    }

    #[test]
    fn the_chat_scale_shrinks_the_hit_area() {
        let scaled = ChatGeometry {
            chat_scale: 0.5,
            ..geometry()
        };
        // x = 40 / 2 - 3 = 17 -> 17 / 0.5 = 34 chat units.
        assert_eq!(hit_row((40, 54), &scaled, 5, 0).map(|hit| hit.x), Some(34));
        let hidden = ChatGeometry {
            chat_scale: 0.0,
            ..geometry()
        };
        assert_eq!(hit_row((40, 54), &hidden, 5, 0), None);
    }

    #[test]
    fn the_piece_under_the_pointer_is_found_by_running_width() {
        let row = [Run::plain("ab"), Run::plain("cd")];
        assert_eq!(piece_at(&row, 0, &six), Some(0));
        assert_eq!(piece_at(&row, 11, &six), Some(0));
        assert_eq!(piece_at(&row, 12, &six), Some(1));
        assert_eq!(piece_at(&row, 24, &six), None);
    }
}
