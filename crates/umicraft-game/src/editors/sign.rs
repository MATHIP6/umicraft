// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sign editor, after `GuiEditSign`.

/// Widest a line may get, in GUI pixels (`getStringWidth(...) <= 90`).
pub const LINE_WIDTH: f32 = 90.0;

/// The four lines of a sign being edited, and the line with the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignEditor {
    lines: [String; 4],
    line: usize,
}

impl SignEditor {
    /// Starts editing a sign that currently reads `lines`, on the first line.
    #[must_use]
    pub fn new(lines: [String; 4]) -> Self {
        Self { lines, line: 0 }
    }

    /// The four lines.
    #[must_use]
    pub fn lines(&self) -> &[String; 4] {
        &self.lines
    }

    /// The line being edited, 0 to 3.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// Moves to the line above (wrapping), like the Up key.
    pub fn up(&mut self) {
        self.line = (self.line + 3) % 4;
    }

    /// Moves to the line below (wrapping), like the Down and Enter keys.
    pub fn down(&mut self) {
        self.line = (self.line + 1) % 4;
    }

    /// Removes the last character of the line.
    pub fn backspace(&mut self) {
        self.lines[self.line].pop();
    }

    /// Types `text` at the end of the line. A character is accepted while the
    /// line is not wider than [`LINE_WIDTH`] before it, and control characters
    /// are dropped (`ChatAllowedCharacters`).
    pub fn insert(&mut self, text: &str, measure: &dyn Fn(&str) -> f32) {
        for character in text.chars().filter(|c| !c.is_control()) {
            if measure(&self.lines[self.line]) <= LINE_WIDTH {
                self.lines[self.line].push(character);
            }
        }
    }

    /// The `Update Sign` lines: each is a chat component, here a JSON string.
    #[must_use]
    pub fn json_lines(&self) -> [String; 4] {
        std::array::from_fn(|i| serde_json::Value::String(self.lines[i].clone()).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn blank() -> SignEditor {
        SignEditor::new(Default::default())
    }

    #[test]
    fn up_and_down_wrap_around() {
        let mut editor = blank();
        editor.up();
        assert_eq!(editor.line(), 3);
        editor.down();
        editor.down();
        assert_eq!(editor.line(), 1);
    }

    #[test]
    fn typing_stops_once_the_line_is_wider_than_ninety() {
        let mut editor = blank();
        editor.insert(&"a".repeat(30), &chars);
        // 15 characters are 90 px: one more is still taken (<= 90 before it).
        assert_eq!(editor.lines()[0].len(), 16);
        editor.insert("b", &chars);
        assert_eq!(editor.lines()[0].len(), 16);
    }

    #[test]
    fn control_characters_are_dropped_and_backspace_pops() {
        let mut editor = blank();
        editor.insert("a\tb\n", &chars);
        assert_eq!(editor.lines()[0], "ab");
        editor.backspace();
        editor.backspace();
        editor.backspace();
        assert_eq!(editor.lines()[0], "");
    }

    #[test]
    fn lines_are_edited_independently_and_sent_as_json_strings() {
        let mut editor = SignEditor::new(["x".into(), "".into(), "".into(), "".into()]);
        editor.down();
        editor.insert("say \"hi\"", &chars);
        assert_eq!(
            editor.json_lines(),
            ["\"x\"", "\"say \\\"hi\\\"\"", "\"\"", "\"\""]
        );
    }
}
