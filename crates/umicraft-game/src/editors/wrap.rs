// SPDX-License-Identifier: MIT OR Apache-2.0

//! Word wrapping at a pixel width (MCP `FontRenderer.listFormattedStringToWidth`).

/// Splits `text` into lines no wider than `width` according to `measure`.
///
/// Newlines always break. Lines break after spaces; a word wider than the
/// width is cut where it stops fitting. Trailing spaces stay on the line
/// they end, like the reference. An empty text is one empty line.
#[must_use]
pub fn wrap(text: &str, width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        wrap_paragraph(paragraph, width, measure, &mut lines);
    }
    lines
}

fn wrap_paragraph(
    paragraph: &str,
    width: f32,
    measure: &dyn Fn(&str) -> f32,
    lines: &mut Vec<String>,
) {
    let mut line = String::new();
    for word in paragraph.split_inclusive(' ') {
        if measure(&format!("{line}{word}")) <= width {
            line.push_str(word);
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for character in word.chars() {
            let mut grown = line.clone();
            grown.push(character);
            if !line.is_empty() && measure(&grown) > width {
                lines.push(std::mem::replace(&mut line, character.to_string()));
            } else {
                line = grown;
            }
        }
    }
    lines.push(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> f32 {
        text.chars().count() as f32
    }

    #[test]
    fn text_that_fits_stays_on_one_line() {
        assert_eq!(wrap("hello", 10.0, &chars), ["hello"]);
        assert_eq!(wrap("", 10.0, &chars), [""]);
    }

    #[test]
    fn lines_break_after_spaces() {
        assert_eq!(wrap("aaa bbb ccc", 8.0, &chars), ["aaa bbb ", "ccc"]);
    }

    #[test]
    fn newlines_always_break() {
        assert_eq!(wrap("a\n\nb", 10.0, &chars), ["a", "", "b"]);
        assert_eq!(wrap("a\n", 10.0, &chars), ["a", ""]);
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut() {
        assert_eq!(wrap("abcdefgh", 3.0, &chars), ["abc", "def", "gh"]);
        assert_eq!(wrap("xx abcdefg", 4.0, &chars), ["xx ", "abcd", "efg"]);
    }
}
