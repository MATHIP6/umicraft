// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sign editor, after `GuiEditSign`: the lines being typed and where the
//! screen draws the sign.

/// Widest a line may get, in GUI pixels (`getStringWidth(...) <= 90`).
pub const LINE_WIDTH: f32 = 90.0;

/// The front of the board in the 64 x 32 sign texture, `(u, v, width,
/// height)`: the north face of `ModelSign.signBoard` (box 24 x 12 x 2 at
/// texture offset 0, 0).
pub const BOARD_TEXELS: (u32, u32, u32, u32) = (2, 2, 24, 12);
/// The front of the post, the north face of `ModelSign.signStick` (box
/// 2 x 14 x 2 at texture offset 0, 14).
pub const POST_TEXELS: (u32, u32, u32, u32) = (2, 16, 2, 14);

/// GUI pixels per block of the sign model (`GlStateManager.scale(-93.75)`).
const GUI_SCALE: f32 = 93.75;
/// The sign renderer's own scale of the model (`0.6666667`).
const MODEL_SCALE: f32 = 2.0 / 3.0;
/// GUI pixels per texel of the model: a texel is a sixteenth of a block.
const TEXEL: f32 = GUI_SCALE * MODEL_SCALE / 16.0;
/// GUI pixels per font pixel of the text (`0.016666668 * 0.6666667` blocks).
pub const TEXT_SCALE: f32 = GUI_SCALE * MODEL_SCALE / 60.0;
/// How far the text plane sits above the model origin (`0.5 * 0.6666667`).
const TEXT_LIFT: f32 = 0.5 * MODEL_SCALE;
/// Top of the board and of the post in model texels (`addBox` y).
const BOARD_TOP: f32 = -14.0;
const POST_TOP: f32 = -2.0;

/// Whether a typed character may go on a sign: not a control character and
/// not the section sign that starts a formatting code
/// (`ChatAllowedCharacters.isAllowedCharacter`).
fn is_allowed(character: char) -> bool {
    character != '\u{a7}' && !character.is_control()
}

/// Where `GuiEditSign.drawScreen` draws the sign, in GUI pixels: `y` from the
/// top of the screen, `x` from its centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignLayout {
    /// The board `[x0, y0, x1, y1]`.
    pub board: [f32; 4],
    /// The post of a standing sign.
    pub post: Option<[f32; 4]>,
    /// Top of the first line of text; the next ones are `10 * TEXT_SCALE`
    /// apart.
    pub first_line_top: f32,
}

impl SignLayout {
    /// The layout of a standing sign or a wall sign.
    ///
    /// The screen translates by `(width / 2, 0)`, scales by 93.75, turns the
    /// sign to face the screen and moves it `1.0625` down, then draws the tile
    /// entity at `(-0.5, -0.75, -0.5)`. The sign renderer lifts its origin by
    /// `0.75 * 0.6666667`, and a wall sign another `-0.3125` (MCP
    /// `GuiEditSign.drawScreen`, `TileEntitySignRenderer.renderTileEntityAt`).
    #[must_use]
    pub fn new(standing: bool) -> Self {
        let wall_drop = if standing { 0.0 } else { 0.3125 };
        // Depth of the model origin under the top of the screen, in blocks.
        let origin = 1.0625 + 0.75 - 0.75 * MODEL_SCALE + wall_drop;
        let origin_y = origin * GUI_SCALE;
        let y = |texels: f32| origin_y + texels * TEXEL;
        let half_board = 12.0 * TEXEL;
        let half_post = TEXEL;
        Self {
            board: [-half_board, y(BOARD_TOP), half_board, y(POST_TOP)],
            post: standing.then(|| [-half_post, y(POST_TOP), half_post, y(POST_TOP + 14.0)]),
            // `drawString(..., j * 10 - 4 * 5)` on the plane lifted by `TEXT_LIFT`.
            first_line_top: (origin - TEXT_LIFT) * GUI_SCALE - 20.0 * TEXT_SCALE,
        }
    }

    /// Top of line `index` of the text.
    #[must_use]
    pub fn line_top(&self, index: usize) -> f32 {
        self.first_line_top + index as f32 * 10.0 * TEXT_SCALE
    }
}

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

    /// Types `text` at the end of the line. A character is accepted when the
    /// line with it is at most [`LINE_WIDTH`] wide (`getStringWidth(s +
    /// typedChar) <= 90`); control characters and `§` are dropped
    /// (`ChatAllowedCharacters.isAllowedCharacter`).
    pub fn insert(&mut self, text: &str, measure: &dyn Fn(&str) -> f32) {
        for character in text.chars().filter(|&c| is_allowed(c)) {
            let line = &mut self.lines[self.line];
            line.push(character);
            if measure(line) > LINE_WIDTH {
                line.pop();
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

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_standing_sign_hangs_its_board_above_its_post() {
        let layout = SignLayout::new(true);
        let [x0, y0, x1, y1] = layout.board;
        assert!(close(x0, -46.875) && close(x1, 46.875));
        assert!(close(y0, 68.36) && close(y1, 115.23), "{y0} {y1}");
        let [_, top, _, bottom] = layout.post.unwrap();
        assert!(close(top, y1) && close(bottom, 169.92), "{bottom}");
        assert!(
            close(layout.first_line_top, 70.96),
            "{}",
            layout.first_line_top
        );
    }

    #[test]
    fn a_wall_sign_is_lower_and_has_no_post() {
        let layout = SignLayout::new(false);
        assert!(layout.post.is_none());
        assert!(close(layout.board[1], 97.66), "{}", layout.board[1]);
        assert!(close(layout.line_top(1), 100.26 + 10.0 * TEXT_SCALE));
    }

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
    fn typing_stops_before_the_line_gets_wider_than_ninety() {
        let mut editor = blank();
        editor.insert(&"a".repeat(30), &chars);
        // 15 characters are 90 px; a 16th would make 96.
        assert_eq!(editor.lines()[0].len(), 15);
        editor.insert("b", &chars);
        assert_eq!(editor.lines()[0].len(), 15);
    }

    #[test]
    fn a_narrow_character_still_fits_after_a_wide_one_is_refused() {
        let widths = |text: &str| -> f32 {
            text.chars()
                .map(|c| match c {
                    'i' => 2.0,
                    'w' => 8.0,
                    _ => 6.0,
                })
                .sum()
        };
        let mut editor = blank();
        editor.insert(&"a".repeat(14), &widths);
        // 84 px: a `w` would make 92, an `i` makes 86.
        editor.insert("wi", &widths);
        assert_eq!(editor.lines()[0], format!("{}i", "a".repeat(14)));
    }

    #[test]
    fn the_section_sign_cannot_be_typed() {
        let mut editor = blank();
        editor.insert("a\u{a7}cb", &chars);
        assert_eq!(editor.lines()[0], "acb");
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
