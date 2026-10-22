// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bitmap font atlas and its layout metrics.
//!
//! Two fonts feed the UI:
//!
//! - [`FontAtlas::fallback`], the original 3×5 font of the M3 overlay. It is
//!   the only font committed to the repository and is used before the user has
//!   installed the game assets.
//! - [`FontAtlas::from_png`], which decodes the user's 1.8
//!   `assets/minecraft/textures/font/ascii.png` at runtime. The asset is read
//!   from the user's client jar and never enters the repository (spec §3
//!   rules 3 and 4, ADR-0006).
//!
//! # Layout
//!
//! [`Glyph::uv_min`] and [`Glyph::uv_max`] select the ink rectangle in the R8
//! atlas; [`Glyph::advance`] is the pen movement, including the one-pixel gap
//! the 1.8 font leaves between glyphs. [`FontAtlas::line_height`] is the
//! baseline-to-baseline distance. Both are in atlas pixels at scale 1.
//!
//! The 1.8 atlas is a 16×16 grid of 8×8 cells in a 128×128 RGBA PNG. The
//! printable ASCII character `c` occupies slot `c` (32..=126). The width of a
//! glyph is the number of non-transparent columns counted from the left plus
//! one pixel of spacing; the space character is forced to four pixels. These
//! rules were derived from the reference renderer consulted under ADR-0001,
//! `mcp: src/minecraft/net/minecraft/client/gui/FontRenderer.java`:
//!
//! - `:35` sets `FONT_HEIGHT = 9`;
//! - `:162-170` reads the 16×16 grid and derives the 8-pixel cell scale of
//!   the 128-pixel asset;
//! - `:172-207` scans each cell from the right for the last non-transparent
//!   column and computes `round(columns * 8 / cell_width) + 1`;
//! - `:177-180` and `:661-664` force a four-pixel space;
//! - `:247-248` maps slot `s` to the cell `(s % 16, s / 16)`;
//! - `:251-263` draws a glyph `advance - 1` pixels wide and returns `advance`
//!   as the pen movement.
//!
//! The decoded atlas is extended by a four-texel white strip on the right so
//! that rectangles have a known opaque texel ([`FontAtlas::white_uv`]). The
//! 3×5 fallback has a dedicated solid-white cell instead.
//!
//! # Fallback policy
//!
//! - `'\n'` starts a new line in [`FontAtlas::measure`] and has no width.
//! - Other control characters have no width and are never drawn.
//! - A character the atlas does not cover is laid out with `'?'` when the
//!   atlas has it, otherwise with `' '`, otherwise with its first glyph. The
//!   same glyph is returned by [`FontAtlas::glyph_or_fallback`], so measuring
//!   and drawing agree.
//! - A zero, negative or non-finite scale makes every metric zero.
//! - Formatting codes (`§`) are not interpreted in M4; they are laid out as
//!   the replacement character.
//!
//! All text indices in this module are `char` indices, never byte offsets.

use thiserror::Error;

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

/// Atlas width of the built-in 3×5 font.
const FALLBACK_WIDTH: u32 = 32;
/// Atlas height of the built-in 3×5 font.
const FALLBACK_HEIGHT: u32 = 48;
/// Cell width of the built-in 3×5 font (3 px glyph plus 1 px spacing).
const FALLBACK_CELL_WIDTH: u32 = 4;
/// Cell height of the built-in 3×5 font (5 px glyph plus 1 px spacing).
const FALLBACK_CELL_HEIGHT: u32 = 6;
/// Columns in the built-in atlas.
const FALLBACK_COLUMNS: u32 = 8;
/// Solid-white cell used for rectangles by the built-in atlas.
const FALLBACK_WHITE_SLOT: usize = 63;
/// Line height of the built-in atlas: a 6 px cell plus 2 px leading.
const FALLBACK_LINE_HEIGHT: f32 = 8.0;

/// The 1.8 font is a 16×16 grid of glyph cells.
const GLYPH_GRID: u32 = 16;
/// The 1.8 `ascii.png` is exactly 128×128 pixels; the reference renderer
/// hard-codes 128-pixel UVs (`FontRenderer.java:254-261`).
const REFERENCE_ATLAS_SIDE: u32 = 128;
/// The first printable ASCII code point, which is also its atlas slot.
const ASCII_FIRST: u32 = 32;
/// The last printable ASCII code point, which is also its atlas slot.
const ASCII_LAST: u32 = 126;
/// Number of printable ASCII glyphs in [`FontAtlas::from_png`].
const ASCII_GLYPH_COUNT: usize = (ASCII_LAST - ASCII_FIRST + 1) as usize;
/// Opaque white columns appended to a decoded atlas for solid rectangles.
const WHITE_STRIP_WIDTH: u32 = 4;
/// `FONT_HEIGHT` of the 1.8 renderer (`FontRenderer.java:35`).
const REFERENCE_LINE_HEIGHT: f32 = 9.0;

/// The eight-byte signature that starts every PNG (PNG specification §5.2).
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Errors returned by [`FontAtlas::from_png`].
#[derive(Debug, Error)]
pub enum FontError {
    /// The input does not start with the eight-byte PNG signature.
    #[error("not a PNG: the eight-byte signature is missing")]
    InvalidSignature,

    /// The PNG container or its compressed pixel data is malformed.
    #[error("PNG decode failed: {0}")]
    Decode(#[from] png::DecodingError),

    /// The image is not the exact 128×128 PNG layout of the 1.8 font.
    #[error("font atlas must be exactly {side}x{side} pixels, got {width}x{height}")]
    InvalidDimensions {
        /// Image width in pixels.
        width: u32,
        /// Image height in pixels.
        height: u32,
        /// The required side length.
        side: u32,
    },

    /// The decoder produced a colour type that cannot be reduced to coverage.
    #[error("unsupported PNG colour type {color_type:?}")]
    UnsupportedColorType {
        /// The colour type reported by the decoder.
        color_type: png::ColorType,
    },

    /// The decoder produced a sample depth other than 8 bits.
    #[error("unsupported PNG bit depth {bit_depth:?}")]
    UnsupportedBitDepth {
        /// The bit depth reported by the decoder.
        bit_depth: png::BitDepth,
    },

    /// The decoded buffer is shorter than the image dimensions require.
    #[error("decoded pixel data is too short: expected {expected} bytes, got {actual}")]
    ShortPixelData {
        /// Number of bytes needed for the whole frame.
        expected: usize,
        /// Number of bytes the decoder produced.
        actual: usize,
    },
}

type FallbackGlyph = (char, [u8; 5]);

/// The original 3×5 glyph bitmaps, upper case only. Lower-case letters are
/// added as aliases when the atlas is built, matching the case-insensitive
/// lookup of the M3 overlay. This table is original Umicraft work.
const FALLBACK_GLYPHS: [FallbackGlyph; 51] = [
    (' ', [0b000, 0b000, 0b000, 0b000, 0b000]),
    ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
    ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
    ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
    ('3', [0b111, 0b001, 0b111, 0b001, 0b111]),
    ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
    ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
    ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
    ('7', [0b111, 0b001, 0b010, 0b010, 0b010]),
    ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
    ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
    ('A', [0b010, 0b101, 0b111, 0b101, 0b101]),
    ('B', [0b110, 0b101, 0b110, 0b101, 0b110]),
    ('C', [0b111, 0b100, 0b100, 0b100, 0b111]),
    ('D', [0b110, 0b101, 0b101, 0b101, 0b110]),
    ('E', [0b111, 0b100, 0b110, 0b100, 0b111]),
    ('F', [0b111, 0b100, 0b110, 0b100, 0b100]),
    ('G', [0b111, 0b100, 0b101, 0b101, 0b111]),
    ('H', [0b101, 0b101, 0b111, 0b101, 0b101]),
    ('I', [0b111, 0b010, 0b010, 0b010, 0b111]),
    ('J', [0b001, 0b001, 0b001, 0b101, 0b111]),
    ('K', [0b101, 0b101, 0b110, 0b101, 0b101]),
    ('L', [0b100, 0b100, 0b100, 0b100, 0b111]),
    ('M', [0b101, 0b111, 0b111, 0b101, 0b101]),
    ('N', [0b101, 0b111, 0b101, 0b101, 0b101]),
    ('O', [0b111, 0b101, 0b101, 0b101, 0b111]),
    ('P', [0b111, 0b101, 0b111, 0b100, 0b100]),
    ('Q', [0b111, 0b101, 0b101, 0b111, 0b001]),
    ('R', [0b111, 0b101, 0b111, 0b110, 0b101]),
    ('S', [0b111, 0b100, 0b111, 0b001, 0b111]),
    ('T', [0b111, 0b010, 0b010, 0b010, 0b010]),
    ('U', [0b101, 0b101, 0b101, 0b101, 0b111]),
    ('V', [0b101, 0b101, 0b101, 0b101, 0b010]),
    ('W', [0b101, 0b101, 0b111, 0b111, 0b101]),
    ('X', [0b101, 0b101, 0b010, 0b101, 0b101]),
    ('Y', [0b101, 0b101, 0b010, 0b010, 0b010]),
    ('Z', [0b111, 0b001, 0b010, 0b100, 0b111]),
    ('.', [0b000, 0b000, 0b000, 0b000, 0b010]),
    (',', [0b000, 0b000, 0b000, 0b010, 0b100]),
    (':', [0b000, 0b010, 0b000, 0b010, 0b000]),
    ('-', [0b000, 0b000, 0b111, 0b000, 0b000]),
    ('+', [0b000, 0b010, 0b111, 0b010, 0b000]),
    ('/', [0b001, 0b001, 0b010, 0b100, 0b100]),
    ('%', [0b101, 0b001, 0b010, 0b100, 0b101]),
    ('(', [0b001, 0b010, 0b010, 0b010, 0b001]),
    (')', [0b100, 0b010, 0b010, 0b010, 0b100]),
    ('[', [0b011, 0b010, 0b010, 0b010, 0b011]),
    (']', [0b110, 0b010, 0b010, 0b010, 0b110]),
    ('_', [0b000, 0b000, 0b000, 0b000, 0b111]),
    ('?', [0b111, 0b001, 0b010, 0b000, 0b010]),
    ('!', [0b010, 0b010, 0b010, 0b000, 0b010]),
];

impl FontAtlas {
    /// Creates an atlas from its parts.
    ///
    /// The caller is responsible for the invariants: `pixels` must hold
    /// `width × height` bytes, the glyph UVs must be inside `0.0..=1.0` and
    /// `white_uv` must point at an opaque texel.
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

    /// Builds the original 3×5 fallback font.
    ///
    /// The bitmaps are the original glyphs of the M3 overlay
    /// (`crates/umicraft-render/src/font.rs`), re-created here so that the UI
    /// crate owns the font instead of the renderer. The atlas is 32×48 with
    /// 4×6 cells; slot 63 is solid white and is exposed through
    /// [`Self::white_uv`] for rectangles. Lower-case letters are aliases of
    /// their upper-case cells, preserving the case-insensitive lookup of the
    /// overlay. The line height is 8 pixels (6 px cell plus 2 px leading) and
    /// every advance is the 4 px cell width.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_ui::FontAtlas;
    ///
    /// let atlas = FontAtlas::fallback();
    /// assert_eq!(atlas.measure("AB", 1.0), 8.0);
    /// assert_eq!(atlas.line_height(1.0), 8.0);
    /// assert!(atlas.glyph('a').is_some());
    /// ```
    #[must_use]
    pub fn fallback() -> Self {
        let mut pixels = vec![0u8; (FALLBACK_WIDTH * FALLBACK_HEIGHT) as usize];
        let mut glyphs = Vec::with_capacity(FALLBACK_GLYPHS.len() + 26);

        for (slot, &(character, rows)) in FALLBACK_GLYPHS.iter().enumerate() {
            let (origin_x, origin_y) = fallback_cell_origin(slot);
            paint_fallback_glyph(&mut pixels, origin_x, origin_y, rows);

            let glyph = Glyph {
                character,
                uv_min: [
                    origin_x as f32 / FALLBACK_WIDTH as f32,
                    origin_y as f32 / FALLBACK_HEIGHT as f32,
                ],
                uv_max: [
                    (origin_x + FALLBACK_CELL_WIDTH) as f32 / FALLBACK_WIDTH as f32,
                    (origin_y + FALLBACK_CELL_HEIGHT) as f32 / FALLBACK_HEIGHT as f32,
                ],
                advance: FALLBACK_CELL_WIDTH as f32,
            };
            glyphs.push(glyph);

            if character.is_ascii_uppercase() {
                glyphs.push(Glyph {
                    character: character.to_ascii_lowercase(),
                    ..glyph
                });
            }
        }

        let (white_x, white_y) = fallback_cell_origin(FALLBACK_WHITE_SLOT);
        for row in 0..FALLBACK_CELL_HEIGHT {
            for column in 0..FALLBACK_CELL_WIDTH {
                pixels[((white_y + row) * FALLBACK_WIDTH + white_x + column) as usize] = 255;
            }
        }

        let white_uv = [
            (white_x + 1) as f32 / FALLBACK_WIDTH as f32,
            (white_y + 1) as f32 / FALLBACK_HEIGHT as f32,
            (white_x + FALLBACK_CELL_WIDTH - 1) as f32 / FALLBACK_WIDTH as f32,
            (white_y + FALLBACK_CELL_HEIGHT - 1) as f32 / FALLBACK_HEIGHT as f32,
        ];

        Self::new(
            FALLBACK_WIDTH,
            FALLBACK_HEIGHT,
            pixels,
            glyphs,
            FALLBACK_LINE_HEIGHT,
            white_uv,
        )
    }

    /// Decodes the user's 1.8 `assets/minecraft/textures/font/ascii.png` into
    /// an R8 coverage atlas.
    ///
    /// The image must be the 128×128 PNG of the 1.8 font: a 16×16 grid of
    /// 8×8 cells. For images with an alpha channel (the 1.8 asset is RGBA) the
    /// alpha sample is the glyph coverage; without alpha, the Rec. 601
    /// luminance is used. Printable ASCII (`' '..='~'`) is mapped to the cell
    /// with the same code point, which is the layout of the reference atlas
    /// (`FontRenderer.java:247-248`). The remaining cells use a
    /// Mojang-specific mapping and are not exposed in M4.
    ///
    /// The returned atlas is four texels wider than the image: the appended
    /// columns are opaque white and back [`Self::white_uv`], so a rectangle
    /// never has to borrow a glyph texel.
    ///
    /// The glyph advances follow the reference derivation
    /// (`FontRenderer.java:172-207`): the number of non-transparent columns
    /// from the left plus one pixel of spacing. A space is forced to four
    /// pixels (`:177-180`, `:661-664`). The line height is the 1.8
    /// `FONT_HEIGHT` of nine pixels (`:35`).
    ///
    /// # Errors
    ///
    /// - [`FontError::InvalidSignature`] when the input is not a PNG at all;
    /// - [`FontError::Decode`] when the container or the pixel stream is
    ///   malformed or truncated;
    /// - [`FontError::InvalidDimensions`] when the image is not the 128×128
    ///   1.8 glyph grid;
    /// - [`FontError::UnsupportedColorType`] /
    ///   [`FontError::UnsupportedBitDepth`] for a frame the decoder cannot
    ///   normalise;
    /// - [`FontError::ShortPixelData`] when the decoder under-delivers.
    ///
    /// The function never panics on malformed input.
    pub fn from_png(bytes: &[u8]) -> Result<Self, FontError> {
        if !bytes.starts_with(&PNG_SIGNATURE) {
            return Err(FontError::InvalidSignature);
        }

        let mut decoder = png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        {
            let header = decoder.read_header_info()?;
            validate_atlas_dimensions(header.width, header.height)?;
        }

        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer)?;
        validate_atlas_dimensions(info.width, info.height)?;

        let (width, height) = (info.width, info.height);
        let coverage = coverage_from_frame(&info, &buffer)?;
        let atlas_width = width + WHITE_STRIP_WIDTH;
        let glyphs = build_ascii_glyphs(width, height, atlas_width, &coverage);
        let (atlas_width, pixels) = append_white_strip(width, height, coverage);
        let white_uv = [
            (width as f32 + 1.0) / atlas_width as f32,
            0.25,
            (width as f32 + 3.0) / atlas_width as f32,
            0.75,
        ];

        Ok(Self::new(
            atlas_width,
            height,
            pixels,
            glyphs,
            REFERENCE_LINE_HEIGHT,
            white_uv,
        ))
    }

    /// True when the atlas has no glyph.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// The glyph for `character`, if the atlas covers it exactly.
    ///
    /// Unknown characters return `None`; use [`Self::glyph_or_fallback`] when
    /// drawing, so that the advance measured by [`Self::advance`] matches the
    /// glyph that is drawn.
    #[must_use]
    pub fn glyph(&self, character: char) -> Option<&Glyph> {
        self.glyphs
            .iter()
            .find(|glyph| glyph.character == character)
    }

    /// The glyph to draw for `character`, applying the replacement policy.
    ///
    /// Control characters (including `'\n'`) return `None`. An uncovered
    /// character maps to `'?'`, then to `' '`, then to the first glyph of the
    /// atlas. This is the same policy [`Self::advance`] uses for measurement.
    #[must_use]
    pub fn glyph_or_fallback(&self, character: char) -> Option<&Glyph> {
        if character.is_control() {
            return None;
        }
        self.glyph(character)
            .or_else(|| self.glyph('?'))
            .or_else(|| self.glyph(' '))
            .or_else(|| self.glyphs.first())
    }

    /// The advance of `character` at `scale`.
    ///
    /// Control characters, an empty atlas, and a zero, negative or non-finite
    /// scale produce `0.0`.
    #[must_use]
    pub fn advance(&self, character: char, scale: f32) -> f32 {
        self.advance_unscaled(character) * sanitize_scale(scale)
    }

    /// The width of the widest line of `text` at `scale`.
    ///
    /// `'\n'` separates lines. Other control characters contribute no width.
    /// Unknown characters use the replacement policy documented on
    /// [`Self::glyph_or_fallback`]. A zero, negative or non-finite scale
    /// returns `0.0`.
    #[must_use]
    pub fn measure(&self, text: &str, scale: f32) -> f32 {
        let scale = sanitize_scale(scale);
        if scale == 0.0 {
            return 0.0;
        }

        let mut line = 0.0f32;
        let mut widest = 0.0f32;
        for character in text.chars() {
            if character == '\n' {
                widest = widest.max(line);
                line = 0.0;
            } else {
                line += self.advance_unscaled(character) * scale;
            }
        }
        widest.max(line)
    }

    /// The line height at `scale`.
    ///
    /// A zero, negative or non-finite scale returns `0.0`.
    #[must_use]
    pub fn line_height(&self, scale: f32) -> f32 {
        self.line_height * sanitize_scale(scale)
    }

    /// The x offset of the caret at `char_index`, relative to the start of its
    /// line, at `scale`.
    ///
    /// `char_index` is a `char` index, not a byte offset. Indices past the end
    /// are clamped to the end of the text. A newline resets the offset to
    /// zero, so the result belongs to the line that contains the caret. Control
    /// characters contribute no width.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_ui::FontAtlas;
    ///
    /// let atlas = FontAtlas::fallback();
    /// assert_eq!(atlas.char_index_to_x("ab", 0, 1.0), 0.0);
    /// assert_eq!(atlas.char_index_to_x("ab", 1, 1.0), 4.0);
    /// assert_eq!(atlas.char_index_to_x("ab", 2, 1.0), 8.0);
    /// ```
    #[must_use]
    pub fn char_index_to_x(&self, text: &str, char_index: usize, scale: f32) -> f32 {
        let scale = sanitize_scale(scale);
        let mut x = 0.0f32;
        for (index, character) in text.chars().enumerate() {
            if index >= char_index {
                break;
            }
            if character == '\n' {
                x = 0.0;
            } else {
                x += self.advance_unscaled(character) * scale;
            }
        }
        x
    }

    /// The `char` index of the boundary nearest to `x` on a single line.
    ///
    /// The text is treated as one line: callers should pass a single-line
    /// string (the text field and the list rows are single-line). `x` before
    /// the start maps to `0`, `x` past the end maps to the number of `char`s,
    /// and a tie between two boundaries maps to the later one. A zero,
    /// negative or non-finite scale maps to `0`; NaN also maps to `0`.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_ui::FontAtlas;
    ///
    /// let atlas = FontAtlas::fallback();
    /// assert_eq!(atlas.x_to_char_index("ab", -10.0, 1.0), 0);
    /// assert_eq!(atlas.x_to_char_index("ab", 100.0, 1.0), 2);
    /// assert_eq!(atlas.x_to_char_index("ab", 5.0, 1.0), 1);
    /// ```
    #[must_use]
    pub fn x_to_char_index(&self, text: &str, x: f32, scale: f32) -> usize {
        let scale = sanitize_scale(scale);
        if x.is_nan() || x <= 0.0 || scale == 0.0 {
            return 0;
        }

        let mut cursor = 0.0f32;
        for (index, character) in text.chars().enumerate() {
            let advance = self.advance_unscaled(character) * scale;
            let midpoint = cursor + advance * 0.5;
            if x < midpoint {
                return index;
            }
            cursor += advance;
        }
        text.chars().count()
    }

    /// The unscaled advance of `character`, shared by the measuring methods.
    fn advance_unscaled(&self, character: char) -> f32 {
        self.glyph_or_fallback(character)
            .map_or(0.0, |glyph| glyph.advance)
    }
}

/// Rejects images that are not the exact 1.8 glyph grid.
fn validate_atlas_dimensions(width: u32, height: u32) -> Result<(), FontError> {
    if width == REFERENCE_ATLAS_SIDE && height == REFERENCE_ATLAS_SIDE {
        Ok(())
    } else {
        Err(FontError::InvalidDimensions {
            width,
            height,
            side: REFERENCE_ATLAS_SIDE,
        })
    }
}

/// Reduces one decoded PNG frame to one byte of coverage per pixel.
///
/// The alpha channel is the coverage when the image has one (the 1.8 atlas is
/// white RGB plus alpha); an RGB or grayscale image uses its luminance, which
/// is what makes a white-on-black atlas work as well.
fn coverage_from_frame(info: &png::OutputInfo, bytes: &[u8]) -> Result<Vec<u8>, FontError> {
    if info.bit_depth != png::BitDepth::Eight {
        return Err(FontError::UnsupportedBitDepth {
            bit_depth: info.bit_depth,
        });
    }

    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(FontError::UnsupportedColorType {
                color_type: info.color_type,
            })
        }
    };

    let pixel_count = (info.width as usize)
        .checked_mul(info.height as usize)
        .ok_or(FontError::InvalidDimensions {
            width: info.width,
            height: info.height,
            side: REFERENCE_ATLAS_SIDE,
        })?;
    let expected = pixel_count
        .checked_mul(channels)
        .ok_or(FontError::InvalidDimensions {
            width: info.width,
            height: info.height,
            side: REFERENCE_ATLAS_SIDE,
        })?;
    let frame = bytes.get(..expected).ok_or(FontError::ShortPixelData {
        expected,
        actual: bytes.len(),
    })?;

    let mut coverage = Vec::with_capacity(pixel_count);
    match info.color_type {
        png::ColorType::Grayscale => coverage.extend_from_slice(frame),
        png::ColorType::GrayscaleAlpha => {
            coverage.extend(frame.chunks_exact(2).map(|pixel| pixel[1]));
        }
        png::ColorType::Rgb => coverage.extend(
            frame
                .chunks_exact(3)
                .map(|pixel| luminance(pixel[0], pixel[1], pixel[2])),
        ),
        png::ColorType::Rgba => {
            coverage.extend(frame.chunks_exact(4).map(|pixel| pixel[3]));
        }
        png::ColorType::Indexed => {
            return Err(FontError::UnsupportedColorType {
                color_type: info.color_type,
            })
        }
    }
    Ok(coverage)
}

/// Rec. 601 luminance, rounded to the nearest byte.
fn luminance(red: u8, green: u8, blue: u8) -> u8 {
    let weighted = u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114;
    ((weighted + 500) / 1000) as u8
}

/// Appends the opaque white strip used by [`FontAtlas::white_uv`].
///
/// `coverage` must hold exactly `width × height` bytes, as built by
/// [`coverage_from_frame`].
fn append_white_strip(width: u32, height: u32, coverage: Vec<u8>) -> (u32, Vec<u8>) {
    let atlas_width = width + WHITE_STRIP_WIDTH;
    let mut pixels = Vec::with_capacity(atlas_width as usize * height as usize);
    for row in coverage.chunks_exact(width as usize) {
        pixels.extend_from_slice(row);
        pixels.extend_from_slice(&[255u8; WHITE_STRIP_WIDTH as usize]);
    }
    (atlas_width, pixels)
}

/// Builds the glyph records for printable ASCII.
///
/// The real 1.8 atlas stores printable ASCII at the slot with the same code
/// point (`FontRenderer.java:237,247-248`). Only those cells are exposed: the
/// extended cells use a Mojang-specific ordering that M4 does not need, and
/// unknown characters use the documented replacement glyph instead.
fn build_ascii_glyphs(width: u32, height: u32, atlas_width: u32, coverage: &[u8]) -> Vec<Glyph> {
    let cell_width = width / GLYPH_GRID;
    let cell_height = height / GLYPH_GRID;
    let mut glyphs = Vec::with_capacity(ASCII_GLYPH_COUNT);

    for character in ' '..='~' {
        let slot = character as u32;
        let origin_x = (slot % GLYPH_GRID) * cell_width;
        let origin_y = (slot / GLYPH_GRID) * cell_height;

        let used = if character == ' ' {
            // The reference forces a four-pixel advance for the space
            // (`FontRenderer.java:177-180`, `:661-664`).
            3
        } else {
            used_columns(coverage, width, origin_x, origin_y, cell_width, cell_height)
        };

        glyphs.push(Glyph {
            character,
            uv_min: [
                origin_x as f32 / atlas_width as f32,
                origin_y as f32 / height as f32,
            ],
            uv_max: [
                (origin_x + used) as f32 / atlas_width as f32,
                (origin_y + cell_height) as f32 / height as f32,
            ],
            advance: used as f32 + 1.0,
        });
    }
    glyphs
}

/// Number of columns, from the left, that contain any non-transparent texel.
///
/// This is the right-to-left scan of `FontRenderer.java:184-205`; trailing
/// transparent columns are trimmed and leading blank columns are kept.
fn used_columns(
    coverage: &[u8],
    stride: u32,
    origin_x: u32,
    origin_y: u32,
    cell_width: u32,
    cell_height: u32,
) -> u32 {
    for column in (0..cell_width).rev() {
        let occupied = (0..cell_height).any(|row| {
            let x = origin_x + column;
            let y = origin_y + row;
            coverage
                .get((y * stride + x) as usize)
                .is_some_and(|value| *value != 0)
        });
        if occupied {
            return column + 1;
        }
    }
    0
}

/// Clamps a scale to a finite, non-negative value.
fn sanitize_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.max(0.0)
    } else {
        0.0
    }
}

/// Top-left texel of a slot in the built-in atlas.
fn fallback_cell_origin(slot: usize) -> (u32, u32) {
    let slot = slot as u32;
    (
        (slot % FALLBACK_COLUMNS) * FALLBACK_CELL_WIDTH,
        (slot / FALLBACK_COLUMNS) * FALLBACK_CELL_HEIGHT,
    )
}

/// Copies one 3×5 bitmap into the built-in atlas.
fn paint_fallback_glyph(pixels: &mut [u8], origin_x: u32, origin_y: u32, rows: [u8; 5]) {
    for (row, pattern) in rows.iter().enumerate() {
        for column in 0..3u32 {
            if pattern & (1u8 << (2 - column)) != 0 {
                let x = origin_x + column;
                let y = origin_y + row as u32;
                pixels[(y * FALLBACK_WIDTH + x) as usize] = 255;
            }
        }
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
                Glyph {
                    character: '?',
                    uv_min: [0.0, 0.5],
                    uv_max: [0.5, 1.0],
                    advance: 5.0,
                },
                Glyph {
                    character: ' ',
                    uv_min: [0.5, 0.5],
                    uv_max: [1.0, 1.0],
                    advance: 3.0,
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
    fn unknown_characters_use_the_replacement_glyph() {
        let atlas = atlas();
        assert_eq!(atlas.measure("AZ", 1.0), 11.0);
        assert!(atlas.glyph('Z').is_none());
        assert_eq!(atlas.glyph_or_fallback('Z'), atlas.glyph('?'));
    }

    #[test]
    fn control_characters_have_no_advance() {
        let atlas = atlas();
        assert_eq!(atlas.advance('\n', 1.0), 0.0);
        assert_eq!(atlas.advance('\t', 1.0), 0.0);
        assert_eq!(atlas.measure("A\nB", 1.0), 6.0);
    }

    #[test]
    fn non_positive_and_non_finite_scales_are_zero() {
        let atlas = atlas();
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(atlas.measure("AB", scale), 0.0);
            assert_eq!(atlas.advance('A', scale), 0.0);
            assert_eq!(atlas.line_height(scale), 0.0);
        }
    }

    #[test]
    fn exposes_the_line_height() {
        assert_eq!(atlas().line_height(1.5), 13.5);
    }

    #[test]
    fn caret_helpers_use_char_indices() {
        let atlas = atlas();
        assert_eq!(atlas.char_index_to_x("AéB", 0, 1.0), 0.0);
        assert_eq!(atlas.char_index_to_x("AéB", 1, 1.0), 6.0);
        assert_eq!(atlas.char_index_to_x("AéB", 2, 1.0), 11.0);
        assert_eq!(atlas.char_index_to_x("AéB", 3, 1.0), 15.0);
        assert_eq!(atlas.x_to_char_index("AéB", 100.0, 1.0), 3);
    }

    #[test]
    fn fallback_has_the_expected_shape_and_glyphs() {
        let atlas = FontAtlas::fallback();
        assert_eq!(atlas.width, 32);
        assert_eq!(atlas.height, 48);
        assert_eq!(atlas.pixels.len(), 32 * 48);
        assert_eq!(atlas.line_height(1.0), 8.0);
        assert_eq!(atlas.measure("hello", 1.0), 20.0);
        let lower = atlas.glyph('a').expect("lower-case alias");
        let upper = atlas.glyph('A').expect("upper-case glyph");
        assert_eq!(lower.uv_min, upper.uv_min);
        assert_eq!(lower.uv_max, upper.uv_max);
        assert_eq!(lower.advance, upper.advance);
        assert!(atlas.glyph('?').is_some());
    }

    #[test]
    fn fallback_white_uv_points_at_opaque_texels() {
        let atlas = FontAtlas::fallback();
        let [u0, v0, u1, v1] = atlas.white_uv;
        for (u, v) in [
            (u0, v0),
            (u1, v0),
            (u0, v1),
            (u1, v1),
            ((u0 + u1) * 0.5, (v0 + v1) * 0.5),
        ] {
            let x = (u * atlas.width as f32).floor() as u32;
            let y = (v * atlas.height as f32).floor() as u32;
            assert_eq!(
                atlas.pixels[(y * atlas.width + x) as usize],
                255,
                "texel at ({x}, {y}) is not white"
            );
        }
    }

    #[test]
    fn fallback_caret_handles_a_multibyte_character() {
        let atlas = FontAtlas::fallback();
        assert_eq!(atlas.char_index_to_x("é", 1, 1.0), 4.0);
        assert_eq!(atlas.x_to_char_index("é", 3.0, 1.0), 1);
        assert_eq!(atlas.x_to_char_index("é", 1.0, 1.0), 0);
    }

    #[test]
    fn used_columns_keeps_leading_and_trims_trailing_blanks() {
        let mut pixels = vec![0u8; 8 * 8];
        pixels[3] = 255; // column 3, row 0
        assert_eq!(super::used_columns(&pixels, 8, 0, 0, 8, 8), 4);
        assert_eq!(super::used_columns(&pixels, 8, 0, 1, 8, 8), 0);

        pixels[0] = 255; // column 0 + column 3
        assert_eq!(super::used_columns(&pixels, 8, 0, 0, 8, 8), 4);
    }

    #[test]
    fn dimension_validation_accepts_only_the_1_8_grid() {
        assert!(super::validate_atlas_dimensions(128, 128).is_ok());
        assert!(super::validate_atlas_dimensions(64, 64).is_err());
        assert!(super::validate_atlas_dimensions(256, 256).is_err());
        assert!(super::validate_atlas_dimensions(128, 64).is_err());
        assert!(super::validate_atlas_dimensions(120, 120).is_err());
    }

    #[test]
    fn luminance_is_rec_601() {
        assert_eq!(super::luminance(255, 255, 255), 255);
        assert_eq!(super::luminance(0, 0, 0), 0);
        assert_eq!(super::luminance(255, 0, 0), 76);
        assert_eq!(super::luminance(0, 255, 0), 150);
        assert_eq!(super::luminance(0, 0, 255), 29);
    }

    #[test]
    fn fallback_bitmaps_are_painted() {
        let atlas = FontAtlas::fallback();
        let glyph = atlas.glyph('A').expect("A is in the fallback font");
        let origin_x = (glyph.uv_min[0] * atlas.width as f32).round() as u32;
        let origin_y = (glyph.uv_min[1] * atlas.height as f32).round() as u32;
        let opaque = (0..5)
            .flat_map(|row| (0..3).map(move |column| (row, column)))
            .filter(|&(row, column)| {
                atlas.pixels[((origin_y + row) * atlas.width + origin_x + column) as usize] != 0
            })
            .count();
        assert_eq!(opaque, 10, "A is 10 set pixels in the 3x5 bitmap");
        assert_eq!(
            atlas.pixels[(origin_y * atlas.width + origin_x + 1) as usize],
            255,
            "A's top row is 010"
        );
        assert_eq!(
            atlas.pixels[(origin_y * atlas.width + origin_x) as usize],
            0,
            "A's top-left pixel is empty"
        );
    }

    #[test]
    fn an_empty_atlas_has_no_fallback() {
        let atlas = FontAtlas::new(1, 1, vec![0], Vec::new(), 0.0, [0.0; 4]);
        assert!(atlas.is_empty());
        assert!(atlas.glyph_or_fallback('A').is_none());
        assert_eq!(atlas.advance('A', 1.0), 0.0);
        assert_eq!(atlas.measure("A", 1.0), 0.0);
    }
}
