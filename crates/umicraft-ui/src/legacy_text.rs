// SPDX-License-Identifier: MIT OR Apache-2.0

//! Text with legacy `§` formatting codes: parsing into [`StyledRun`]s and
//! wrapping that carries the active format over a line break.
//!
//! The server list description, the version text and the player sample of a
//! server keep the colours the server chose (`FontRenderer.renderStringAtPos`
//! for the codes, `FontRenderer.listFormattedStringToWidth` and
//! `getFormatFromString` for the wrapping; MCP 1.8, ADR-0001). A colour code
//! resets the style flags, `§r` returns to the base colour, and an unknown
//! code is white, as vanilla finds no colour index for it.

use crate::draw::Color;
use crate::styled_text::StyledRun;
use crate::text::FontAtlas;

/// The section sign that starts a formatting code.
const SECTION: char = '\u{a7}';
/// The colours of `§0`..`§f` (`FontRenderer.colorCode`).
const PALETTE: [u32; 16] = [
    0x00_00_00, 0x00_00_AA, 0x00_AA_00, 0x00_AA_AA, 0xAA_00_00, 0xAA_00_AA, 0xFF_AA_00, 0xAA_AA_AA,
    0x55_55_55, 0x55_55_FF, 0x55_FF_55, 0x55_FF_FF, 0xFF_55_55, 0xFF_55_FF, 0xFF_FF_55, 0xFF_FF_FF,
];

/// The active format: a colour index (`None` = the base colour) and flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Format {
    color: Option<usize>,
    obfuscated: bool,
    bold: bool,
    strikethrough: bool,
    underlined: bool,
    italic: bool,
}

impl Format {
    /// Applies one code letter (case-insensitive).
    fn apply(&mut self, code: char) {
        match code.to_ascii_lowercase() {
            digit @ ('0'..='9' | 'a'..='f') => {
                let index = digit.to_digit(16).unwrap_or(15) as usize;
                *self = Self {
                    color: Some(index),
                    ..Self::default()
                };
            }
            'k' => self.obfuscated = true,
            'l' => self.bold = true,
            'm' => self.strikethrough = true,
            'n' => self.underlined = true,
            'o' => self.italic = true,
            'r' => *self = Self::default(),
            // Vanilla finds no index for an unknown code and takes white.
            _ => {
                *self = Self {
                    color: Some(15),
                    ..Self::default()
                }
            }
        }
    }

    /// The codes that rebuild this format after a reset (`getFormatFromString`).
    fn prefix(self) -> String {
        let mut out = String::new();
        if let Some(color) = self.color {
            out.extend([SECTION, char::from_digit(color as u32, 16).unwrap_or('f')]);
        }
        let flags = [
            (self.obfuscated, 'k'),
            (self.bold, 'l'),
            (self.strikethrough, 'm'),
            (self.underlined, 'n'),
            (self.italic, 'o'),
        ];
        for (_, letter) in flags.iter().filter(|(on, _)| *on) {
            out.extend([SECTION, *letter]);
        }
        out
    }

    fn run(self, text: String, base: Color) -> StyledRun {
        let color = self.color.map_or(base, |index| {
            let rgb = PALETTE[index];
            [
                ((rgb >> 16) & 0xFF) as f32 / 255.0,
                ((rgb >> 8) & 0xFF) as f32 / 255.0,
                (rgb & 0xFF) as f32 / 255.0,
                base[3],
            ]
        });
        StyledRun {
            text,
            color,
            bold: self.bold,
            italic: self.italic,
            underlined: self.underlined,
            strikethrough: self.strikethrough,
            obfuscated: self.obfuscated,
        }
    }
}

/// Splits `text` into styled runs; `base` is the colour before any code and
/// after `§r`. A trailing lone `§` is dropped.
///
/// ```
/// use umicraft_ui::legacy_text::parse_legacy;
///
/// let runs = parse_legacy("a§lb", [1.0; 4]);
/// assert_eq!(runs.len(), 2);
/// assert!(runs[1].bold);
/// ```
#[must_use]
pub fn parse_legacy(text: &str, base: Color) -> Vec<StyledRun> {
    let mut runs = Vec::new();
    let mut format = Format::default();
    let mut current = String::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != SECTION {
            current.push(character);
            continue;
        }
        if !current.is_empty() {
            runs.push(format.run(std::mem::take(&mut current), base));
        }
        if let Some(code) = chars.next() {
            format.apply(code);
        }
    }
    if !current.is_empty() {
        runs.push(format.run(current, base));
    }
    runs
}

/// Splits `text` into lines no wider than `max_width` GUI units, breaking at
/// spaces (or inside a word wider than a line) and at `\n`. Every line after
/// the first starts with the codes of the format active at the break.
///
/// Bold glyphs count one unit more, as drawn.
#[must_use]
pub fn wrap_legacy(font: &FontAtlas, text: &str, max_width: f32) -> Vec<String> {
    let mut wrapper = Wrapper::new(font, max_width);
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        match character {
            SECTION => {
                if let Some(code) = chars.next() {
                    wrapper.code(code);
                }
            }
            '\n' => wrapper.newline(),
            _ => wrapper.glyph(character),
        }
    }
    wrapper.finish()
}

/// A possible break: the byte offset of the space in the line and the format
/// in force there.
#[derive(Clone, Copy)]
struct BreakPoint {
    at: usize,
    format: Format,
}

/// The state of [`wrap_legacy`].
struct Wrapper<'a> {
    font: &'a FontAtlas,
    max_width: f32,
    lines: Vec<String>,
    line: String,
    width: f32,
    format: Format,
    space: Option<BreakPoint>,
}

impl<'a> Wrapper<'a> {
    fn new(font: &'a FontAtlas, max_width: f32) -> Self {
        Self {
            font,
            max_width,
            lines: Vec::new(),
            line: String::new(),
            width: 0.0,
            format: Format::default(),
            space: None,
        }
    }

    fn code(&mut self, code: char) {
        self.format.apply(code);
        self.line.extend([SECTION, code]);
    }

    fn advance(&self, character: char, format: Format) -> f32 {
        self.font.advance(character, 1.0) + if format.bold { 1.0 } else { 0.0 }
    }

    fn glyph(&mut self, character: char) {
        let advance = self.advance(character, self.format);
        if self.width + advance > self.max_width && self.width > 0.0 {
            self.wrap();
        }
        if character == ' ' {
            self.space = Some(BreakPoint {
                at: self.line.len(),
                format: self.format,
            });
        }
        self.line.push(character);
        self.width += advance;
    }

    /// Ends the line at the last space, or here when the word is too long.
    fn wrap(&mut self) {
        let Some(point) = self.space.take() else {
            self.newline();
            return;
        };
        let tail = self.line.split_off(point.at + 1);
        self.line.pop();
        self.lines.push(std::mem::take(&mut self.line));
        self.line = point.format.prefix();
        self.width = 0.0;
        self.format = point.format;
        let mut chars = tail.chars();
        while let Some(character) = chars.next() {
            if character == SECTION {
                if let Some(code) = chars.next() {
                    self.code(code);
                }
            } else {
                self.width += self.advance(character, self.format);
                self.line.push(character);
            }
        }
    }

    fn newline(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
        self.line = self.format.prefix();
        self.width = 0.0;
        self.space = None;
    }

    fn finish(mut self) -> Vec<String> {
        self.lines.push(self.line);
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Color = [0.5, 0.5, 0.5, 1.0];

    fn font() -> FontAtlas {
        FontAtlas::fallback()
    }

    #[test]
    fn colours_flags_and_resets_become_runs() {
        let runs = parse_legacy("§dPrisme §lBold§r plain", BASE);
        let texts: Vec<_> = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["Prisme ", "Bold", " plain"]);
        assert_eq!(runs[0].color, [1.0, 85.0 / 255.0, 1.0, 1.0]);
        assert!(runs[1].bold && !runs[0].bold);
        assert_eq!(runs[2].color, BASE);
        assert!(!runs[2].bold);
    }

    #[test]
    fn a_colour_code_clears_the_flags() {
        let runs = parse_legacy("§lA§cB", BASE);
        assert!(runs[0].bold);
        assert!(!runs[1].bold);
    }

    #[test]
    fn a_trailing_section_sign_is_dropped() {
        let runs = parse_legacy("ab§", BASE);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "ab");
    }

    #[test]
    fn the_wrap_breaks_at_spaces_and_carries_the_format() {
        let lines = wrap_legacy(&font(), "§aword word word", 40.0);
        assert_eq!(lines, ["§aword word", "§aword"]);
    }

    #[test]
    fn codes_have_no_width() {
        let plain = wrap_legacy(&font(), "abc def", 1000.0);
        let coded = wrap_legacy(&font(), "§a§labc def", 1000.0);
        assert_eq!(plain.len(), 1);
        assert_eq!(coded.len(), 1);
    }

    #[test]
    fn a_word_wider_than_a_line_is_cut() {
        let lines = wrap_legacy(&font(), "abcdefghij", 12.0);
        assert!(lines.len() >= 3, "{lines:?}");
    }

    #[test]
    fn newline_starts_a_new_line_with_the_same_format() {
        let lines = wrap_legacy(&font(), "§ca\nb", 100.0);
        assert_eq!(lines, ["§ca", "§cb"]);
    }
}
