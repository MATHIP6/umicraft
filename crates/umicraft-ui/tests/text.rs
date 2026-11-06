// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the font atlas and its layout metrics.
//!
//! The PNG fixtures are generated in this file: the repository never contains
//! a Mojang asset (spec §3 rules 3 and 4). The real 1.8 `ascii.png` is only
//! read by the ignored test at the end, from the path in `UMICRAFT_FONT_PNG`.

use std::error::Error;

use umicraft_ui::font_grid::GRID_CHARS;
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

/// Grid slot of `character` in the reference `charList` order.
fn slot_of(character: char) -> u32 {
    let slot = GRID_CHARS
        .chars()
        .position(|candidate| candidate == character)
        .expect("the character is in the grid");
    slot as u32
}

/// Paints `columns` opaque white columns in the cell of `character`.
fn paint_glyph(pixels: &mut [u8], character: char, columns: u32) {
    let slot = slot_of(character);
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
    paint_glyph(&mut pixels, 'é', 5);
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
    assert_eq!(atlas.glyphs.len(), 247);
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
    assert!(atlas.glyph('é').is_some(), "the accent cells are exposed");
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
    assert_eq!(atlas.glyphs.len(), 247);
    assert_eq!(atlas.glyph('A').map(|glyph| glyph.advance), Some(6.0));
    assert_eq!(atlas.glyph('W').map(|glyph| glyph.advance), Some(6.0));
    assert_eq!(atlas.glyph('i').map(|glyph| glyph.advance), Some(2.0));
    assert_eq!(atlas.glyph('.').map(|glyph| glyph.advance), Some(2.0));
    assert_eq!(atlas.glyph(' ').map(|glyph| glyph.advance), Some(4.0));
    assert_eq!(atlas.glyph('@').map(|glyph| glyph.advance), Some(7.0));
    assert_eq!(atlas.measure("hello", 1.0), 24.0);
    Ok(())
}

#[test]
fn accented_and_box_characters_get_their_own_glyphs() -> Result<(), Box<dyn Error>> {
    let atlas = FontAtlas::from_png(&synthetic_png()?)?;
    for character in ['é', 'ê', 'ç', 'ñ', 'à', 'Ç', 'ü', '│', '█', 'π', '°'] {
        let glyph = atlas.glyph(character).expect("the grid has the character");
        assert_ne!(
            atlas.glyph_or_fallback(character),
            atlas.glyph('?'),
            "{character} must not render as ?"
        );
        assert_eq!(glyph.character, character);
    }
    // The cell of 'é' is slot 130: column 2, row 8 of the 16x16 grid.
    let e_acute = atlas.glyph('é').expect("é is in the grid");
    assert!((e_acute.uv_min[0] - 16.0 / 132.0).abs() < f32::EPSILON);
    assert!((e_acute.uv_min[1] - 64.0 / 128.0).abs() < f32::EPSILON);
    assert_eq!(e_acute.advance, 6.0);
    Ok(())
}

#[test]
fn unknown_characters_still_fall_back_to_the_question_mark() -> Result<(), Box<dyn Error>> {
    let atlas = FontAtlas::from_png(&synthetic_png()?)?;
    assert!(
        atlas.glyph('€').is_none(),
        "the euro sign is not in the 1.8 grid"
    );
    assert_eq!(atlas.glyph_or_fallback('€'), atlas.glyph('?'));
    assert_eq!(atlas.glyph_or_fallback('\u{4e2d}'), atlas.glyph('?'));
    Ok(())
}

/// Regression: a scoreboard line showed `Prismas: 0 ?` where vanilla draws the
/// symbol from its Unicode pages.
#[test]
fn unicode_pages_draw_the_characters_outside_the_grid() -> Result<(), Box<dyn Error>> {
    let grid = FontAtlas::from_png(&synthetic_png()?)?;
    let star = '\u{2726}';
    let (page, cell) = (0x27_u8, 0x26_usize);
    let mut sizes = vec![0u8; 65_536];
    // Texel columns 0 to 11 of the cell, which `getCharWidth` counts as a
    // whole cell: 16 / 2 + 1.
    sizes[usize::from(page) * 256 + cell] = 0x0b;
    let page_png = encode_png(256, 256, png::ColorType::Grayscale, &vec![255u8; 256 * 256])?;
    let atlas = grid.with_unicode(&sizes, |index| (index == page).then(|| page_png.clone()));

    let glyph = atlas.glyph(star).expect("the page gives the star a glyph");
    assert_eq!(glyph.scale, 0.5, "drawn at half size");
    assert_eq!(atlas.advance(star, 1.0), 9.0);
    let width = (glyph.uv_max[0] - glyph.uv_min[0]) * atlas.width as f32;
    assert!((width - 11.98).abs() < 1e-3, "`renderUnicodeChar` quad: {width}");
    let (u, v) = (glyph.uv_min[0] + 1e-4, glyph.uv_min[1] + 1e-4);
    assert_eq!(sample_uv(&atlas, u, v), 255, "the UV points into the page");

    // The grid glyphs are unchanged in look and advance; only their UVs moved.
    for character in ['A', 'B', '?', 'é'] {
        assert_eq!(
            atlas.advance(character, 1.0),
            grid.advance(character, 1.0),
            "{character}"
        );
    }
    let (before, after) = (grid.glyph('A').unwrap(), atlas.glyph('A').unwrap());
    assert_eq!(
        sample_uv(&atlas, after.uv_min[0], after.uv_min[1]),
        sample_uv(&grid, before.uv_min[0], before.uv_min[1])
    );
    // With the pages loaded a character that has no glyph is what vanilla
    // makes of it: no width and nothing drawn, not a `?`.
    for missing in ['\u{2728}', '\u{4e2d}'] {
        assert!(atlas.glyph_or_fallback(missing).is_none(), "{missing}");
        assert_eq!(atlas.advance(missing, 1.0), 0.0, "{missing}");
        assert_eq!(atlas.pen_advance(missing), 0.0, "{missing}");
    }
    // Without the table the atlas is returned as it is.
    assert_eq!(grid.with_unicode(&[], |_| None), grid);
    Ok(())
}

/// `getCharWidth` counts a glyph that ends after column 7 as a whole cell,
/// but drawing moves the pen by what `renderUnicodeChar` returns.
#[test]
fn a_wide_unicode_glyph_measures_a_cell_and_draws_its_own_width() -> Result<(), Box<dyn Error>> {
    let grid = FontAtlas::from_png(&synthetic_png()?)?;
    let mut sizes = vec![0u8; 65_536];
    // First column 1, last column 10: 10 texels wide.
    sizes[0x2700 + 0x27] = 0x1a;
    // 0x80 and up is a negative Java byte whose `>>> 4` is a huge column.
    sizes[0x2700 + 0x28] = 0x8f;
    let page_png = encode_png(256, 256, png::ColorType::Grayscale, &vec![255u8; 256 * 256])?;
    let atlas = grid.with_unicode(&sizes, |index| (index == 0x27).then(|| page_png.clone()));

    let wide = '\u{2727}';
    assert_eq!(atlas.advance(wide, 1.0), 9.0, "getCharWidth: (15 + 1 - 0) / 2 + 1");
    assert_eq!(atlas.glyph(wide).map(|glyph| glyph.pen), Some(6.0), "(10 + 1 - 1) / 2 + 1");
    assert_eq!(atlas.pen_advance(wide), 6.0);
    assert!(atlas.glyph('\u{2728}').is_none(), "a size from 0x80 is ignored");
    assert!(atlas.is_unicode(wide));
    assert!(!atlas.is_unicode('A'));
    assert!(atlas.grid_glyphs().iter().all(|glyph| !atlas.is_unicode(glyph.character)));
    // The grid keeps measuring and drawing the same.
    assert_eq!(atlas.pen_advance('A'), atlas.advance('A', 1.0));
    Ok(())
}

/// `renderStringAtPos` pulls the shadow of a Unicode glyph back onto it, so
/// only the grid glyphs have a visible offset.
#[test]
fn the_shadow_of_a_unicode_glyph_is_not_offset() -> Result<(), Box<dyn Error>> {
    use umicraft_ui::styled_text::{draw_styled, StyledRun};
    use umicraft_ui::{DrawCommand, DrawList};

    let grid = FontAtlas::from_png(&synthetic_png()?)?;
    let mut sizes = vec![0u8; 65_536];
    sizes[0x2700 + 0x26] = 0x0b;
    let page_png = encode_png(256, 256, png::ColorType::Grayscale, &vec![255u8; 256 * 256])?;
    let atlas = grid.with_unicode(&sizes, |index| (index == 0x27).then(|| page_png.clone()));

    let mut list = DrawList::new();
    let runs = [StyledRun::plain("A\u{2726}", [1.0; 4])];
    let advance = draw_styled(&mut list, &atlas, [10.0, 20.0], &runs, 3.0, true, 0);
    let drawn: Vec<([f32; 2], String)> = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { position, text, .. } => Some((*position, text.clone())),
            _ => None,
        })
        .collect();
    // Shadow pass first: the grid glyph one unit (3 px) down and right, the
    // star on its own spot; then the text pass.
    let pen = atlas.pen_advance('A') * 3.0;
    assert_eq!(
        drawn,
        vec![
            ([13.0, 23.0], "A".to_owned()),
            ([10.0 + pen, 20.0], "\u{2726}".to_owned()),
            ([10.0, 20.0], "A\u{2726}".to_owned()),
        ]
    );
    assert_eq!(advance, pen + atlas.pen_advance('\u{2726}') * 3.0);
    Ok(())
}

/// `randomStyle && j != -1`: obfuscated text scrambles the grid only.
#[test]
fn obfuscated_text_never_turns_into_a_unicode_glyph() -> Result<(), Box<dyn Error>> {
    use umicraft_ui::styled_text::{draw_styled, StyledRun};
    use umicraft_ui::{DrawCommand, DrawList};

    let grid = FontAtlas::from_png(&synthetic_png()?)?;
    let mut sizes = vec![0u8; 65_536];
    // Every character of the page has the width of `A`.
    sizes[0x2700..0x2800].fill(0x03);
    let page_png = encode_png(256, 256, png::ColorType::Grayscale, &vec![255u8; 256 * 256])?;
    let atlas = grid.with_unicode(&sizes, |index| (index == 0x27).then(|| page_png.clone()));

    let mut run = StyledRun::plain("AAAA\u{2726}", [1.0; 4]);
    run.obfuscated = true;
    for tick in 0..50 {
        let mut list = DrawList::new();
        draw_styled(&mut list, &atlas, [0.0, 0.0], std::slice::from_ref(&run), 1.0, false, tick);
        for command in list.commands() {
            if let DrawCommand::Text { text, .. } = command {
                assert!(
                    text.chars().take(4).all(|c| !atlas.is_unicode(c)),
                    "scrambled into a Unicode glyph: {text:?}"
                );
                assert!(text.ends_with('\u{2726}'), "a Unicode glyph is kept: {text:?}");
            }
        }
    }
    Ok(())
}

/// Regression: the server list showed `Prisme B?ta` for `Prisme Bêta`.
#[test]
fn prisme_beta_measures_with_the_real_glyph() -> Result<(), Box<dyn Error>> {
    let atlas = FontAtlas::from_png(&synthetic_png()?)?;
    let question = atlas.advance('?', 1.0);
    let accent = atlas.advance('ê', 1.0);
    assert_ne!(question, accent);
    assert_eq!(
        atlas.measure("Bêta", 1.0),
        atlas.advance('B', 1.0) + accent + atlas.advance('t', 1.0) + atlas.advance('a', 1.0)
    );
    Ok(())
}
