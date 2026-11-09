// SPDX-License-Identifier: MIT OR Apache-2.0

//! The brand font: a TrueType face rasterized at runtime into a
//! [`FontAtlas`] (ADR-0056).
//!
//! Umicraft's own screens use a sans-serif face (Inter SemiBold, SIL OFL 1.1)
//! next to the game's bitmap font. [`rasterize`] draws printable ASCII and the
//! Latin-1 Supplement with `fontdue` into the same [`FontAtlas`] type the
//! bitmap font uses, so measuring and drawing share one code path.
//!
//! # Atlas conventions
//!
//! The draw pass places a glyph quad at the pen position with the size of its
//! UV rectangle and has no per-glyph bearing or baseline (see `glyph_quad` in
//! `umicraft-render`). Each cell therefore spans the pen origin to the pen
//! origin plus the advance horizontally, and the whole line (ascent plus
//! descent) vertically; the ink sits inside the cell at its bearing and
//! baseline offset. Advances are whole pixels because the pass moves the pen
//! by `pen.trunc()`. Kerning is not applied.
//!
//! Cells are separated by a transparent border so that the linear sampling
//! used for this atlas never bleeds a neighbour in. A 4×4 opaque block gives
//! rectangles their white texel ([`FontAtlas::white_uv`]); its centre lies
//! between four white texels, so bilinear sampling stays pure white.
//!
//! Callers draw brand text at `scale = wanted_px / BRAND_PIXEL_SIZE`.

use fontdue::{Font, FontSettings};
use thiserror::Error;

use crate::icons::{self, Icon};
use crate::text::{FontAtlas, Glyph};

/// The size, in pixels per em, at which the brand face is rasterized. Large
/// enough that text drawn at 12 to 40 px (scale 0.25 to 0.85) stays crisp
/// with linear filtering.
pub const BRAND_PIXEL_SIZE: f32 = 48.0;

/// Transparent border around every cell, against linear-filter bleeding.
const PADDING: u32 = 2;
/// Width of the atlas texture; rows of cells wrap at this width.
const ATLAS_WIDTH: u32 = 1024;
/// Side of the opaque block used for rectangles.
const WHITE_SIDE: u32 = 4;
/// The largest accepted rasterization size; keeps a cell inside the atlas.
const MAX_PIXEL_SIZE: f32 = 256.0;
/// Soft hyphen: invisible and without a glyph in most fonts.
const SOFT_HYPHEN: char = '\u{AD}';

/// Errors returned by [`rasterize`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BrandFontError {
    /// The bytes are not a TrueType or OpenType font.
    #[error("brand font cannot be parsed: {0}")]
    Parse(&'static str),
    /// The font has no horizontal line metrics.
    #[error("brand font has no horizontal line metrics")]
    NoLineMetrics,
    /// The requested size is not a finite value in `(0, 256]`.
    #[error("brand font pixel size must be in (0, {MAX_PIXEL_SIZE}]")]
    InvalidPixelSize,
    /// A glyph is wider than the atlas.
    #[error("glyph {0:?} does not fit the brand atlas")]
    GlyphTooLarge(char),
}

/// One rasterized glyph cell: the ink placed on its pen-to-pen, full-line box.
struct Cell {
    character: char,
    advance: u32,
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

/// Where a block of the atlas sits, in texels.
#[derive(Clone, Copy)]
struct Placement {
    x: u32,
    y: u32,
}

/// Rasterizes `ttf` at `pixel_size` into a [`FontAtlas`] for the brand face.
///
/// The atlas covers printable ASCII (32..=126) and the Latin-1 Supplement
/// (U+00A1..=U+00FF, without the soft hyphen), skipping characters the font
/// lacks. Glyph advances are whole pixels at `pixel_size`; draw the result at
/// `scale = wanted_px / pixel_size`.
///
/// # Errors
///
/// [`BrandFontError::Parse`] for bytes that are not a font,
/// [`BrandFontError::NoLineMetrics`] for a font without horizontal metrics,
/// [`BrandFontError::InvalidPixelSize`] for a size outside `(0, 256]` and
/// [`BrandFontError::GlyphTooLarge`] when a glyph exceeds the atlas width.
///
/// # Example
///
/// ```no_run
/// use umicraft_ui::brand_font::{rasterize, BRAND_PIXEL_SIZE};
///
/// # fn demo(ttf: &[u8]) -> Result<(), umicraft_ui::brand_font::BrandFontError> {
/// let atlas = rasterize(ttf, BRAND_PIXEL_SIZE)?;
/// let width = atlas.measure("Umicraft", 20.0 / BRAND_PIXEL_SIZE);
/// # let _ = width;
/// # Ok(())
/// # }
/// ```
pub fn rasterize(ttf: &[u8], pixel_size: f32) -> Result<FontAtlas, BrandFontError> {
    if !(pixel_size.is_finite() && pixel_size > 0.0 && pixel_size <= MAX_PIXEL_SIZE) {
        return Err(BrandFontError::InvalidPixelSize);
    }
    let font = Font::from_bytes(ttf, FontSettings::default()).map_err(BrandFontError::Parse)?;
    let metrics = font
        .horizontal_line_metrics(pixel_size)
        .ok_or(BrandFontError::NoLineMetrics)?;
    let baseline = metrics.ascent.ceil() as i32;
    let cell_height = (baseline + (-metrics.descent).ceil() as i32).max(1) as u32;

    let mut cells: Vec<Cell> = covered_characters()
        .filter(|&c| c == ' ' || font.lookup_glyph_index(c) != 0)
        .map(|c| render_cell(&font, c, pixel_size, baseline, cell_height))
        .collect();
    cells.extend(Icon::ALL.map(|icon| icon_cell(icon, pixel_size, cell_height)));

    let (placements, white, height) = pack(&cells)?;
    let mut pixels = vec![0u8; ATLAS_WIDTH as usize * height as usize];
    let mut glyphs = Vec::with_capacity(cells.len());
    for (cell, at) in cells.iter().zip(&placements) {
        blit(&mut pixels, *at, cell.width, &cell.coverage);
        glyphs.push(glyph(cell, *at, height));
    }
    blit(
        &mut pixels,
        white,
        WHITE_SIDE,
        &[255u8; (WHITE_SIDE * WHITE_SIDE) as usize],
    );

    let line_height = (metrics.ascent - metrics.descent + metrics.line_gap).ceil();
    Ok(FontAtlas::new(
        ATLAS_WIDTH,
        height,
        pixels,
        glyphs,
        line_height.max(cell_height as f32),
        white_uv(white, height),
    ))
}

/// The characters of the atlas in ascending order, so that the space comes
/// first as [`FontAtlas::glyph_or_fallback`] expects.
fn covered_characters() -> impl Iterator<Item = char> {
    (0x20u8..=0x7E)
        .map(char::from)
        .chain((0xA1u8..=0xFF).map(char::from))
        .filter(|&c| c != SOFT_HYPHEN)
}

/// Rasterizes `character` and places it on its full-line cell.
fn render_cell(font: &Font, character: char, size: f32, baseline: i32, height: u32) -> Cell {
    let (metrics, bitmap) = font.rasterize(character, size);
    let advance = metrics.advance_width.round().max(1.0) as u32;
    // A negative bearing would draw left of the pen; the cell cannot, so the
    // ink is shifted right (a sub-pixel at the brand size).
    let left = metrics.xmin.max(0) as u32;
    let width = advance.max(left + metrics.width as u32);
    let top = baseline - (metrics.ymin + metrics.height as i32);

    let mut coverage = vec![0u8; (width * height) as usize];
    for (row, source) in bitmap.chunks_exact(metrics.width.max(1)).enumerate() {
        let Some(y) = u32::try_from(top + row as i32).ok().filter(|y| *y < height) else {
            continue;
        };
        let start = (y * width + left) as usize;
        coverage[start..start + source.len()].copy_from_slice(source);
    }
    Cell {
        character,
        advance,
        width,
        height,
        coverage,
    }
}

/// The cell of `icon`: a square as wide as the em, centred on the line so that
/// drawing it like a letter puts it on the middle of the line.
fn icon_cell(icon: Icon, pixel_size: f32, height: u32) -> Cell {
    let side = pixel_size.round().max(1.0) as u32;
    let ink = icons::rasterize(icon, side);
    let top = height.saturating_sub(side) / 2;
    let mut coverage = vec![0u8; (side * height) as usize];
    for (row, source) in ink.chunks_exact(side as usize).enumerate() {
        let y = top as usize + row;
        if y < height as usize {
            let start = y * side as usize;
            coverage[start..start + source.len()].copy_from_slice(source);
        }
    }
    Cell {
        character: icon.character(),
        advance: side,
        width: side,
        height,
        coverage,
    }
}

/// Shelf-packs the cells and the white block into rows of [`ATLAS_WIDTH`].
///
/// Returns the placement of each cell, of the white block, and the atlas
/// height.
fn pack(cells: &[Cell]) -> Result<(Vec<Placement>, Placement, u32), BrandFontError> {
    let (mut x, mut y, mut row_height) = (PADDING, PADDING, 0);
    let mut place = |width: u32, height: u32| -> Option<Placement> {
        if width + 2 * PADDING > ATLAS_WIDTH {
            return None;
        }
        if x + width + PADDING > ATLAS_WIDTH {
            x = PADDING;
            y += row_height + PADDING;
            row_height = 0;
        }
        let at = Placement { x, y };
        x += width + PADDING;
        row_height = row_height.max(height);
        Some(at)
    };
    let placements = cells
        .iter()
        .map(|cell| {
            place(cell.width, cell.height).ok_or(BrandFontError::GlyphTooLarge(cell.character))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let white = place(WHITE_SIDE, WHITE_SIDE).ok_or(BrandFontError::GlyphTooLarge(' '))?;
    Ok((placements, white, y + row_height + PADDING))
}

/// Copies a `width`-wide coverage block into the atlas at `at`.
fn blit(pixels: &mut [u8], at: Placement, width: u32, coverage: &[u8]) {
    for (row, source) in coverage.chunks_exact(width as usize).enumerate() {
        let start = (at.y as usize + row) * ATLAS_WIDTH as usize + at.x as usize;
        pixels[start..start + source.len()].copy_from_slice(source);
    }
}

/// The glyph record of a placed cell.
fn glyph(cell: &Cell, at: Placement, atlas_height: u32) -> Glyph {
    let (w, h) = (ATLAS_WIDTH as f32, atlas_height as f32);
    Glyph {
        character: cell.character,
        uv_min: [at.x as f32 / w, at.y as f32 / h],
        uv_max: [
            (at.x + cell.width) as f32 / w,
            (at.y + cell.height) as f32 / h,
        ],
        advance: cell.advance as f32,
        pen: cell.advance as f32,
        scale: 1.0,
    }
}

/// UV rectangle of the opaque block.
fn white_uv(at: Placement, atlas_height: u32) -> [f32; 4] {
    let (w, h) = (ATLAS_WIDTH as f32, atlas_height as f32);
    [
        at.x as f32 / w,
        at.y as f32 / h,
        (at.x + WHITE_SIDE) as f32 / w,
        (at.y + WHITE_SIDE) as f32 / h,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTER: &[u8] =
        include_bytes!("../../umicraft-assets/client-pack/umicraft/font/Inter-SemiBold.ttf");

    fn atlas() -> FontAtlas {
        rasterize(INTER, BRAND_PIXEL_SIZE).expect("the committed font rasterizes")
    }

    /// Texel rectangle `[x0, y0, x1, y1]` of a UV rectangle.
    fn texels(atlas: &FontAtlas, min: [f32; 2], max: [f32; 2]) -> [usize; 4] {
        let (w, h) = (atlas.width as f32, atlas.height as f32);
        [
            (min[0] * w).round() as usize,
            (min[1] * h).round() as usize,
            (max[0] * w).round() as usize,
            (max[1] * h).round() as usize,
        ]
    }

    #[test]
    fn text_has_a_positive_width_that_scales_linearly() {
        let atlas = atlas();
        let one = atlas.measure("U", 1.0);
        assert!(one > 0.0);
        assert!(atlas.measure("Umicraft", 1.0) > one);
        assert_eq!(
            atlas.measure("Umicraft", 0.5) * 2.0,
            atlas.measure("Umicraft", 1.0)
        );
    }

    #[test]
    fn french_accents_have_their_own_glyphs() {
        let atlas = atlas();
        for character in "éèêëàâçîïôùûüÉÀÇ«»".chars() {
            assert!(atlas.glyph(character).is_some(), "{character} is missing");
        }
    }

    #[test]
    fn the_line_box_holds_every_cell() {
        let atlas = atlas();
        assert!(atlas.line_height(1.0) >= BRAND_PIXEL_SIZE);
        for glyph in &atlas.glyphs {
            let height = (glyph.uv_max[1] - glyph.uv_min[1]) * atlas.height as f32;
            assert!(height <= atlas.line_height + 0.5);
        }
    }

    #[test]
    fn glyph_ink_is_rasterized_and_uvs_are_normalised() {
        let atlas = atlas();
        assert_eq!(atlas.pixels.len(), (atlas.width * atlas.height) as usize);
        for glyph in &atlas.glyphs {
            assert!(glyph.uv_min[0] >= 0.0 && glyph.uv_max[0] <= 1.0);
            assert!(glyph.uv_min[1] >= 0.0 && glyph.uv_max[1] <= 1.0);
        }
        let h = atlas.glyph('H').expect("H is covered");
        let [x0, y0, x1, y1] = texels(&atlas, h.uv_min, h.uv_max);
        let inked = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| atlas.pixels[y * atlas.width as usize + x] > 200)
            .count();
        assert!(inked > 100);
    }

    #[test]
    fn every_icon_is_a_glyph_with_ink_as_wide_as_the_em() {
        let atlas = atlas();
        for icon in Icon::ALL {
            let glyph = atlas
                .glyph(icon.character())
                .expect("the icon is in the atlas");
            assert_eq!(glyph.advance, BRAND_PIXEL_SIZE);
            let [x0, y0, x1, y1] = texels(&atlas, glyph.uv_min, glyph.uv_max);
            let inked = (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| atlas.pixels[y * atlas.width as usize + x] > 200)
                .count();
            assert!(inked > 60, "{icon:?} has no ink in the atlas");
        }
    }

    #[test]
    fn the_white_texel_is_an_opaque_block() {
        let atlas = atlas();
        let [u0, v0, u1, v1] = atlas.white_uv;
        let [x0, y0, x1, y1] = texels(&atlas, [u0, v0], [u1, v1]);
        assert_eq!((x1 - x0, y1 - y0), (4, 4));
        for y in y0..y1 {
            for x in x0..x1 {
                assert_eq!(atlas.pixels[y * atlas.width as usize + x], 255);
            }
        }
    }

    #[test]
    fn the_atlas_starts_with_transparent_padding() {
        let atlas = atlas();
        let border = ATLAS_WIDTH as usize * PADDING as usize;
        assert!(atlas.pixels[..border].iter().all(|&p| p == 0));
    }

    #[test]
    fn space_is_first_and_unknown_characters_fall_back() {
        let atlas = atlas();
        assert_eq!(atlas.glyphs[0].character, ' ');
        assert_eq!(atlas.measure("\u{4e2d}", 1.0), atlas.measure("?", 1.0));
    }

    #[test]
    fn garbage_bytes_are_an_error() {
        assert!(matches!(
            rasterize(b"not a font at all", BRAND_PIXEL_SIZE),
            Err(BrandFontError::Parse(_))
        ));
        assert!(rasterize(&[], BRAND_PIXEL_SIZE).is_err());
    }

    #[test]
    fn invalid_sizes_are_rejected() {
        for size in [0.0, -4.0, f32::NAN, f32::INFINITY, 1000.0] {
            assert_eq!(
                rasterize(INTER, size).err(),
                Some(BrandFontError::InvalidPixelSize)
            );
        }
    }
}
