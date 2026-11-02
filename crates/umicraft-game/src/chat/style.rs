// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chat colours and styles, and the legacy `§` formatting codes.
//!
//! Sources: MCP `EnumChatFormatting` (the sixteen colours, their names and
//! the code pattern `§[0-9A-FK-OR]`), `ChatStyle` (the style flags, inherited
//! from the parent when unset) and `FontRenderer.renderStringAtPos` (what a
//! `§` code does while text is drawn).

/// The section sign that starts a legacy formatting code.
pub const SECTION: char = '\u{a7}';

/// One of the sixteen chat colours, by its formatting index (`0` black to
/// `f` white).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(u8);

impl Color {
    /// White, the default colour of chat text.
    pub const WHITE: Self = Self(15);

    /// The colour of a legacy code character such as `'c'`.
    #[must_use]
    pub fn from_code(code: char) -> Option<Self> {
        code.to_digit(16)
            .and_then(|digit| u8::try_from(digit).ok())
            .map(Self)
    }

    /// The colour with this JSON name, such as `"dark_red"`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        const NAMES: [&str; 16] = [
            "black",
            "dark_blue",
            "dark_green",
            "dark_aqua",
            "dark_red",
            "dark_purple",
            "gold",
            "gray",
            "dark_gray",
            "blue",
            "green",
            "aqua",
            "red",
            "light_purple",
            "yellow",
            "white",
        ];
        NAMES
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
            .and_then(|index| u8::try_from(index).ok())
            .map(Self)
    }

    /// The formatting index, `0..=15`.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// The RGB value of the colour (`FontRenderer.colorCode`, without the
    /// darker shadow variants).
    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        const TABLE: [[u8; 3]; 16] = [
            [0x00, 0x00, 0x00],
            [0x00, 0x00, 0xAA],
            [0x00, 0xAA, 0x00],
            [0x00, 0xAA, 0xAA],
            [0xAA, 0x00, 0x00],
            [0xAA, 0x00, 0xAA],
            [0xFF, 0xAA, 0x00],
            [0xAA, 0xAA, 0xAA],
            [0x55, 0x55, 0x55],
            [0x55, 0x55, 0xFF],
            [0x55, 0xFF, 0x55],
            [0x55, 0xFF, 0xFF],
            [0xFF, 0x55, 0x55],
            [0xFF, 0x55, 0xFF],
            [0xFF, 0xFF, 0x55],
            [0xFF, 0xFF, 0xFF],
        ];
        TABLE[(self.0 & 15) as usize]
    }
}

/// The resolved style of a run of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    /// The text colour, white when `None`.
    pub color: Option<Color>,
    /// Bold text.
    pub bold: bool,
    /// Italic text, drawn as a sheared glyph.
    pub italic: bool,
    /// Underlined text.
    pub underlined: bool,
    /// Struck-through text.
    pub strikethrough: bool,
    /// Text drawn as random glyphs of the same width.
    pub obfuscated: bool,
}

impl Style {
    /// Applies the legacy code `code` that follows a `§`, the way
    /// `FontRenderer.renderStringAtPos` does.
    ///
    /// `k l m n o` switch one flag on; `r` resets everything; every other
    /// character is a colour code, which also clears the flags, and an
    /// unknown one selects white (`i1 < 0 -> 15`).
    pub fn apply_code(&mut self, code: char) {
        match code.to_ascii_lowercase() {
            'k' => self.obfuscated = true,
            'l' => self.bold = true,
            'm' => self.strikethrough = true,
            'n' => self.underlined = true,
            'o' => self.italic = true,
            'r' => *self = Self::default(),
            other => {
                *self = Self {
                    color: Some(Color::from_code(other).unwrap_or(Color::WHITE)),
                    ..Self::default()
                };
            }
        }
    }
}

/// Whether `code` is one of the characters `EnumChatFormatting` recognises
/// after a `§` (`[0-9A-FK-OR]`, any case).
fn is_formatting_code(code: char) -> bool {
    matches!(code.to_ascii_lowercase(), '0'..='9' | 'a'..='f' | 'k'..='o' | 'r')
}

/// The text without its formatting codes
/// (`EnumChatFormatting.getTextWithoutFormattingCodes`).
///
/// An unknown code such as `§z` is kept, exactly like the reference pattern.
#[must_use]
pub fn strip_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == SECTION && chars.peek().copied().is_some_and(is_formatting_code) {
            chars.next();
        } else {
            out.push(character);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_map_to_the_font_table() {
        assert_eq!(Color::from_name("red").unwrap().rgb(), [0xFF, 0x55, 0x55]);
        assert_eq!(Color::from_code('0').unwrap().rgb(), [0, 0, 0]);
        assert_eq!(Color::from_name("RED"), Color::from_name("red"));
        assert_eq!(Color::from_name("purple"), None);
        assert_eq!(Color::from_code('g'), None);
    }

    #[test]
    fn every_flag_code_switches_its_flag_on_and_keeps_the_colour() {
        let mut style = Style {
            color: Color::from_code('c'),
            ..Style::default()
        };
        for code in ['k', 'l', 'm', 'n', 'o'] {
            style.apply_code(code);
        }
        assert!(style.obfuscated && style.bold && style.strikethrough);
        assert!(style.underlined && style.italic);
        assert_eq!(style.color, Color::from_code('c'));
    }

    #[test]
    fn reset_clears_everything_and_a_colour_clears_the_flags() {
        let mut style = Style::default();
        style.apply_code('l');
        style.apply_code('a');
        assert!(!style.bold, "a colour code resets the other styles");
        style.apply_code('o');
        style.apply_code('R');
        assert_eq!(style, Style::default());
    }

    #[test]
    fn an_unknown_code_is_white() {
        let mut style = Style::default();
        style.apply_code('z');
        assert_eq!(style.color, Some(Color::WHITE));
    }

    #[test]
    fn stripping_removes_only_the_codes_the_reference_pattern_knows() {
        assert_eq!(strip_codes("\u{a7}cRed \u{a7}LBold"), "Red Bold");
        assert_eq!(strip_codes("a\u{a7}zb"), "a\u{a7}zb");
        assert_eq!(strip_codes("end\u{a7}"), "end\u{a7}");
    }
}
