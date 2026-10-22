// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bitmap font atlas and its layout metrics.
//!
//! The atlas is plain data: an R8 coverage bitmap plus one [`Glyph`] record
//! per character. `umicraft-render` uploads the bitmap once and draws quads
//! from the UVs; it never parses a PNG or a font format. The 1.8 client font
//! is loaded at runtime from the user's client jar (spec M4); the built-in
//! 3×5 fallback is original work and is the only font committed to the
//! repository (spec §3).
//!
//! The loader lives with the text workstream (W2). This module freezes the
//! data shape that the widgets and the renderer share.

/// One glyph in a [`FontAtlas`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// The character this glyph renders.
    pub character: char,
    /// Minimum UV in the atlas, normalised to `0.0..=1.0`.
    pub uv_min: [f32; 2],
    /// Maximum UV in the atlas, normalised to `0.0..=1.0`.
    pub uv_max: [f32; 2],
    /// Horizontal advance in atlas pixels at scale 1.
    pub advance: f32,
}

/// An R8 bitmap font atlas and the metrics needed to lay out text.
#[derive(Debug, Clone, PartialEq)]
pub struct FontAtlas {
    /// Atlas width in texels.
    pub width: u32,
    /// Atlas height in texels.
    pub height: u32,
    /// Row-major R8 coverage, `width × height` bytes.
    pub pixels: Vec<u8>,
    /// The glyphs known to this atlas.
    pub glyphs: Vec<Glyph>,
    /// Distance between two baselines at scale 1, in atlas pixels.
    pub line_height: f32,
    /// UV rectangle of a solid white texel, used for rectangles.
    pub white_uv: [f32; 4],
}

impl FontAtlas {
    /// Creates an atlas from its parts.
    #[must_use]
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        glyphs: Vec<Glyph>,
        line_height: f32,
        white_uv: [f32; 4],
    ) -> Self {
        Self {
            width,
            height,
            pixels,
            glyphs,
            line_height,
            white_uv,
        }
    }

    /// True when the atlas has no glyph.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// The glyph for `character`, if the atlas covers it.
    #[must_use]
    pub fn glyph(&self, character: char) -> Option<&Glyph> {
        self.glyphs
            .iter()
            .find(|glyph| glyph.character == character)
    }

    /// The advance of `character` at `scale`, or zero when it is missing.
    #[must_use]
    pub fn advance(&self, character: char, scale: f32) -> f32 {
        self.glyph(character)
            .map_or(0.0, |glyph| glyph.advance * scale)
    }

    /// The width of the widest line of `text` at `scale`.
    #[must_use]
    pub fn measure(&self, text: &str, scale: f32) -> f32 {
        let mut line: f32 = 0.0;
        let mut widest: f32 = 0.0;
        for character in text.chars() {
            if character == '\n' {
                widest = widest.max(line);
                line = 0.0;
            } else {
                line += self.advance(character, scale);
            }
        }
        widest.max(line)
    }

    /// The line height at `scale`.
    #[must_use]
    pub fn line_height(&self, scale: f32) -> f32 {
        self.line_height * scale
    }
}

#[cfg(test)]
mod tests {
    use super::{FontAtlas, Glyph};

    fn atlas() -> FontAtlas {
        FontAtlas::new(
            16,
            16,
            vec![0; 256],
            vec![
                Glyph {
                    character: 'A',
                    uv_min: [0.0, 0.0],
                    uv_max: [0.5, 0.5],
                    advance: 6.0,
                },
                Glyph {
                    character: 'B',
                    uv_min: [0.5, 0.0],
                    uv_max: [1.0, 0.5],
                    advance: 4.0,
                },
            ],
            9.0,
            [0.9, 0.9, 1.0, 1.0],
        )
    }

    #[test]
    fn measures_each_line_and_keeps_the_widest() {
        let atlas = atlas();
        assert_eq!(atlas.measure("AB", 1.0), 10.0);
        assert_eq!(atlas.measure("AB\nB", 1.0), 10.0);
        assert_eq!(atlas.measure("AB\nA", 1.0), 10.0);
        assert_eq!(atlas.measure("B\nA", 1.0), 6.0);
        assert_eq!(atlas.measure("AB", 2.0), 20.0);
    }

    #[test]
    fn unknown_characters_have_no_advance() {
        let atlas = atlas();
        assert_eq!(atlas.measure("AZ", 1.0), 6.0);
        assert!(atlas.glyph('Z').is_none());
    }

    #[test]
    fn exposes_the_line_height() {
        assert_eq!(atlas().line_height(1.5), 13.5);
    }
}
