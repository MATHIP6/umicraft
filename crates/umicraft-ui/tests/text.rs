// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the font atlas and its layout metrics.
//!
//! The PNG fixtures are generated in this file: the repository never contains
//! a Mojang asset (spec §3 rules 3 and 4). The real 1.8 `ascii.png` is only
//! read by the ignored test at the end, from the path in `UMICRAFT_FONT_PNG`.

use std::error::Error;

use umicraft_ui::text::FontError;
use umicraft_ui::FontAtlas;

/// Side of the synthetic atlas and of the real 1.8 atlas.
const SIZE: u32 = 128;
/// Side of one glyph cell in the synthetic atlas.
const CELL: u32 = SIZE / 16;
/// Opaque columns that `from_png` appends for rectangles.
const WHITE_STRIP: u32 = 4;

/// Encodes an 8-bit PNG with the given colour type.
fn encode_png(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    pixels: &[u8],
) -> Result<Vec<u8>, png::EncodingError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(pixels)?;
    }
    Ok(bytes)
}

/// Paints `columns` opaque white columns in the cell of `character`.
fn paint_glyph(pixels: &mut [u8], character: char, columns: u32) {
    let slot = character as u32;
    let cell_x = (slot % 16) * CELL;
    let cell_y = (slot / 16) * CELL;
    for row in 0..CELL {
        for column in 0..columns {
            let x = cell_x + column;
            let y = cell_y + row;
            let offset = ((y * SIZE + x) * 4) as usize;
            pixels[offset..offset + 4].fill(255);
        }
    }
}

/// A 128×128 RGBA atlas with known glyph widths:
/// `A` = 4 px, `B` = 9 px, `?` = 6 px, `i` = 2 px, space = 4 px.
fn synthetic_png() -> Result<Vec<u8>, png::EncodingError> {
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    paint_glyph(&mut pixels, 'A', 3);
    paint_glyph(&mut pixels, 'B', 8);
    paint_glyph(&mut pixels, '?', 5);
    paint_glyph(&mut pixels, 'i', 1);
    encode_png(SIZE, SIZE, png::ColorType::Rgba, &pixels)
}

/// A blank RGBA PNG used by the dimension tests.
fn blank_rgba_png(width: u32, height: u32) -> Result<Vec<u8>, png::EncodingError> {
    let pixels = vec![0u8; (width * height * 4) as usize];
    encode_png(width, height, png::ColorType::Rgba, &pixels)
}

/// Reads one byte of the atlas at a normalised UV coordinate.
fn sample_uv(atlas: &FontAtlas, u: f32, v: f32) -> u8 {
    let x = ((u * atlas.width as f32).floor() as u32).min(atlas.width - 1);
    let y = ((v * atlas.height as f32).floor() as u32).min(atlas.height - 1);
    atlas.pixels[(y * atlas.width + x) as usize]
}

#[test]
fn synthetic_png_decodes_to_coverage_and_glyphs() -> Result<(), Box<dyn Error>> {
    let png = synthetic_png()?;
    let atlas = FontAtlas::from_png(&png)?;

    assert_eq!(atlas.width, SIZE + WHITE_STRIP);
    assert_eq!(atlas.height, SIZE);
    assert_eq!(atlas.pixels.len(), ((SIZE + WHITE_STRIP) * SIZE) as usize);
    assert_eq!(atlas.glyphs.len(), 95);
    assert_eq!(atlas.line_height, 9.0);
    assert_eq!(atlas.line_height(2.0), 18.0);

    let a = atlas.glyph('A').expect("A is printable ASCII");
    assert_eq!(a.advance, 4.0);
    assert!((a.uv_min[0] - 8.0 / 132.0).abs() < f32::EPSILON);
    assert!((a.uv_min[1] - 32.0 / 128.0).abs() < f32::EPSILON);
    assert!((a.uv_max[0] - 11.0 / 132.0).abs() < f32::EPSILON);
    assert!((a.uv_max[1] - 40.0 / 128.0).abs() < f32::EPSILON);

    let b = atlas.glyph('B').expect("B is printable ASCII");
    assert_eq!(b.advance, 9.0);
    assert!((b.uv_max[0] - 24.0 / 132.0).abs() < f32::EPSILON);

    let i = atlas.glyph('i').expect("i is printable ASCII");
    assert_eq!(i.advance, 2.0);
    let question = atlas.glyph('?').expect("? is printable ASCII");
    assert_eq!(question.advance, 6.0);
    let space = atlas.glyph(' ').expect("space is printable ASCII");
    assert_eq!(space.advance, 4.0);

    assert!(atlas.glyph('~').is_some());
    assert!(atlas.glyph('\u{7f}').is_none());
    assert!(atlas.glyph('é').is_none(), "only ASCII slots are exposed");
    assert!(atlas.glyph_or_fallback('\u{7f}').is_none());
    Ok(())
}

#[test]
fn decoded_atlas_measures_and_wraps_lines() -> Result<(), Box<dyn Error>> {
    let atlas = FontAtlas::from_png(&synthetic_png()?)?;

    assert_eq!(atlas.measure("", 1.0), 0.0);
    assert_eq!(atlas.measure("A", 1.0), 4.0);
    assert_eq!(atlas.measure("AB", 1.0), 13.0);
    assert_eq!(atlas.measure("A\nB", 1.0), 9.0);
    assert_eq!(atlas.measure("B\nA", 1.0), 9.0);
    assert_eq!(atlas.measure("A\n", 1.0), 4.0);
    assert_eq!(atlas.measure("  ", 1.0), 8.0);
    assert_eq!(atlas.measure("AB", 2.0), 26.0);

    // Unknown characters use '?', control characters have no width.
    assert_eq!(atlas.measure("☃", 1.0), 6.0);
    assert_eq!(atlas.advance('☃', 1.0), 6.0);
    assert_eq!(atlas.advance('\n', 1.0), 0.0);
    assert_eq!(atlas.advance('\t', 1.0), 0.0);

    for character in ['A', 'Z', '☃', 'é', '§'] {
        let expected = atlas
            .glyph_or_fallback(character)
            .map_or(0.0, |glyph| glyph.advance);
        assert_eq!(atlas.advance(character, 1.0), expected);
    }
    Ok(())
}

#[test]
fn decoded_atlas_has_an_opaque_white_strip() -> Result<(), Box<dyn Error>> {
    let atlas = FontAtlas::from_png(&synthetic_png()?)?;

    for y in 0..atlas.height {
        for x in SIZE..atlas.width {
            assert_eq!(
                atlas.pixels[(y * atlas.width + x) as usize],
                255,
                "white strip texel ({x}, {y})"
            );
        }
    }

    let [u0, v0, u1, v1] = atlas.white_uv;
    assert!(u0 >= SIZE as f32 / atlas.width as f32);
    assert!(u1 <= 1.0);
    for (u, v) in [
        (u0, v0),
        (u1, v0),
        (u0, v1),
        (u1, v1),
        ((u0 + u1) * 0.5, (v0 + v1) * 0.5),
    ] {
        assert_eq!(sample_uv(&atlas, u, v), 255, "white_uv sample ({u}, {v})");
    }
    Ok(())
}

#[test]
fn bad_signature_is_rejected() {
    assert!(matches!(
        FontAtlas::from_png(b"not a png"),
        Err(FontError::InvalidSignature)
    ));
    assert!(matches!(
        FontAtlas::from_png(&[]),
        Err(FontError::InvalidSignature)
    ));
}

#[test]
fn truncated_png_is_rejected_without_panicking() -> Result<(), Box<dyn Error>> {
    let png = synthetic_png()?;

    for cut in [8, 16, 32, png.len() / 2] {
        assert!(
            FontAtlas::from_png(&png[..cut]).is_err(),
            "truncated PNG of {cut} bytes decoded"
        );
    }

    // Every prefix must return or fail cleanly; a panic fails the test.
    for cut in 0..png.len() {
        let _ = FontAtlas::from_png(&png[..cut]);
    }
    Ok(())
}

#[test]
fn invalid_dimensions_are_rejected() -> Result<(), Box<dyn Error>> {
    let not_square = blank_rgba_png(128, 64)?;
    assert!(matches!(
        FontAtlas::from_png(&not_square),
        Err(FontError::InvalidDimensions {
            width: 128,
            height: 64,
            side: SIZE
        })
    ));

    let wrong_side = blank_rgba_png(64, 64)?;
    assert!(matches!(
        FontAtlas::from_png(&wrong_side),
        Err(FontError::InvalidDimensions {
            width: 64,
            height: 64,
            side: SIZE
        })
    ));

    let not_a_grid = blank_rgba_png(120, 120)?;
    assert!(matches!(
        FontAtlas::from_png(&not_a_grid),
        Err(FontError::InvalidDimensions {
            width: 120,
            height: 120,
            side: SIZE
        })
    ));

    let too_small = blank_rgba_png(8, 8)?;
    assert!(matches!(
        FontAtlas::from_png(&too_small),
        Err(FontError::InvalidDimensions {
            width: 8,
            height: 8,
            side: SIZE
        })
    ));
    Ok(())
}

#[test]
fn grayscale_and_rgb_coverages_are_supported() -> Result<(), Box<dyn Error>> {
    let mut gray = vec![0u8; (SIZE * SIZE) as usize];
    let a_slot = 'A' as u32;
    let a_x = (a_slot % 16) * CELL;
    let a_y = (a_slot / 16) * CELL;
    for row in 0..CELL {
        for column in 0..3 {
            gray[((a_y + row) * SIZE + a_x + column) as usize] = 200;
        }
    }
    let atlas = FontAtlas::from_png(&encode_png(SIZE, SIZE, png::ColorType::Grayscale, &gray)?)?;
    assert_eq!(atlas.glyph('A').map(|glyph| glyph.advance), Some(4.0));
    assert_eq!(
        atlas.pixels[(a_y * atlas.width + a_x) as usize],
        200,
        "grayscale coverage is copied"
    );

    let mut rgb = vec![0u8; (SIZE * SIZE * 3) as usize];
    for row in 0..CELL {
        for column in 0..5 {
            let offset = (((a_y + row) * SIZE + a_x + column) * 3) as usize;
            rgb[offset..offset + 3].fill(255);
        }
    }
    let atlas = FontAtlas::from_png(&encode_png(SIZE, SIZE, png::ColorType::Rgb, &rgb)?)?;
    assert_eq!(atlas.glyph('A').map(|glyph| glyph.advance), Some(6.0));
    assert_eq!(atlas.pixels[(a_y * atlas.width + a_x) as usize], 255);
    Ok(())
}

#[test]
fn caret_helpers_use_char_boundaries() -> Result<(), Box<dyn Error>> {
    let atlas = FontAtlas::from_png(&synthetic_png()?)?;
    // "A" advances 4 px, "é" falls back to '?' (6 px) and "B" advances 9 px.
    let text = "AéB";
    assert_eq!(atlas.char_index_to_x(text, 0, 1.0), 0.0);
    assert_eq!(atlas.char_index_to_x(text, 1, 1.0), 4.0);
    assert_eq!(atlas.char_index_to_x(text, 2, 1.0), 10.0);
    assert_eq!(atlas.char_index_to_x(text, 3, 1.0), 19.0);
    assert_eq!(atlas.char_index_to_x(text, 99, 1.0), 19.0);

    assert_eq!(atlas.x_to_char_index(text, -5.0, 1.0), 0);
    assert_eq!(atlas.x_to_char_index(text, 0.0, 1.0), 0);
    assert_eq!(atlas.x_to_char_index(text, 1.9, 1.0), 0);
    assert_eq!(atlas.x_to_char_index(text, 2.0, 1.0), 1);
    assert_eq!(atlas.x_to_char_index(text, 6.9, 1.0), 1);
    assert_eq!(atlas.x_to_char_index(text, 7.0, 1.0), 2);
    assert_eq!(atlas.x_to_char_index(text, 100.0, 1.0), 3);

    // A multi-byte character is one char index, not two byte offsets.
    assert_eq!(atlas.char_index_to_x("é", 0, 1.0), 0.0);
    assert_eq!(atlas.char_index_to_x("é", 1, 1.0), 6.0);
    assert_eq!(atlas.x_to_char_index("é", 2.9, 1.0), 0);
    assert_eq!(atlas.x_to_char_index("é", 3.0, 1.0), 1);
    Ok(())
}

#[test]
fn caret_x_is_relative_to_the_line_start() {
    let atlas = FontAtlas::fallback();
    assert_eq!(atlas.char_index_to_x("ab\ncd", 3, 1.0), 0.0);
    assert_eq!(atlas.char_index_to_x("ab\ncd", 4, 1.0), 4.0);
    assert_eq!(atlas.char_index_to_x("ab\ncd", 5, 1.0), 8.0);
}

#[test]
fn zero_and_negative_scales_disable_every_metric() {
    let atlas = FontAtlas::fallback();
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(atlas.measure("AB", scale), 0.0);
        assert_eq!(atlas.advance('A', scale), 0.0);
        assert_eq!(atlas.line_height(scale), 0.0);
    }
    assert_eq!(atlas.x_to_char_index("AB", 10.0, 0.0), 0);
}

#[test]
fn fallback_atlas_is_original_and_usable() {
    let atlas = FontAtlas::fallback();
    assert_eq!(atlas.width, 32);
    assert_eq!(atlas.height, 48);
    assert_eq!(atlas.pixels.len(), 32 * 48);
    assert_eq!(atlas.line_height(1.0), 8.0);
    assert_eq!(atlas.measure("hello", 1.0), 20.0);
    assert_eq!(atlas.measure("hi\nthere", 1.0), 20.0);
    assert_eq!(atlas.glyph(' ').map(|glyph| glyph.advance), Some(4.0));
    assert!(atlas.glyph('a').is_some());
    assert!(atlas.glyph('Z').is_some());
    assert!(atlas.glyph('☃').is_none());
    assert_eq!(atlas.advance('☃', 1.0), 4.0);

    let [u0, v0, u1, v1] = atlas.white_uv;
    assert_eq!(sample_uv(&atlas, (u0 + u1) * 0.5, (v0 + v1) * 0.5), 255);
}

/// Reads the user's own 1.8 `ascii.png` from `UMICRAFT_FONT_PNG`.
///
/// The test is ignored by default because the asset must never enter the
/// repository (spec §3 rules 3 and 4). Run it with:
///
/// ```sh
/// UMICRAFT_FONT_PNG=/path/to/assets/minecraft/textures/font/ascii.png \
///     cargo test -p umicraft-ui --test text -- --ignored
/// ```
#[test]
#[ignore = "reads a user-supplied Mojang asset from UMICRAFT_FONT_PNG"]
fn decodes_a_real_client_font() -> Result<(), Box<dyn Error>> {
    let Ok(path) = std::env::var("UMICRAFT_FONT_PNG") else {
        return Ok(());
    };
    let bytes = std::fs::read(path)?;
    let atlas = FontAtlas::from_png(&bytes)?;

    assert_eq!((atlas.width, atlas.height), (SIZE + WHITE_STRIP, SIZE));
    assert_eq!(atlas.line_height, 9.0);
    assert_eq!(atlas.glyphs.len(), 95);
    assert_eq!(atlas.glyph('A').map(|glyph| glyph.advance), Some(6.0));
    assert_eq!(atlas.glyph('W').map(|glyph| glyph.advance), Some(6.0));
    assert_eq!(atlas.glyph('i').map(|glyph| glyph.advance), Some(2.0));
    assert_eq!(atlas.glyph('.').map(|glyph| glyph.advance), Some(2.0));
    assert_eq!(atlas.glyph(' ').map(|glyph| glyph.advance), Some(4.0));
    assert_eq!(atlas.glyph('@').map(|glyph| glyph.advance), Some(7.0));
    assert_eq!(atlas.measure("hello", 1.0), 24.0);
    Ok(())
}
