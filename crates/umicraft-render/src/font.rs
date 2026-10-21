// SPDX-License-Identifier: MIT OR Apache-2.0

//! A tiny original 3×5 bitmap font for the debug overlay.
//!
//! The atlas is a 32×48 R8 texture: 8 columns × 8 rows of 4×6 cells, with the
//! glyph in the top-left 3×5 pixels. Slot 63 is solid white and doubles as the
//! texture for solid overlay rectangles. Unknown characters map to space.

/// Atlas width in texels.
pub const ATLAS_WIDTH: u32 = 32;
/// Atlas height in texels.
pub const ATLAS_HEIGHT: u32 = 48;
/// Cell width in texels (3 px glyph + 1 px spacing).
pub const CELL_WIDTH: u32 = 4;
/// Cell height in texels (5 px glyph + 1 px spacing).
pub const CELL_HEIGHT: u32 = 6;
/// Columns in the atlas.
pub const ATLAS_COLUMNS: u32 = 8;
/// Solid-white cell used for overlay rectangles.
pub const WHITE_SLOT: usize = 63;

type Glyph = (char, [u8; 5]);

const GLYPHS: [Glyph; 51] = [
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

/// Builds the R8 font atlas (row-major, top-left origin).
#[must_use]
pub fn atlas() -> Vec<u8> {
    let mut data = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT) as usize];
    for (slot, (_, rows)) in GLYPHS.iter().enumerate() {
        let (base_x, base_y) = cell_origin(slot);
        for (row, pattern) in rows.iter().enumerate() {
            for column in 0..3u32 {
                if pattern & (1 << (2 - column)) != 0 {
                    let x = base_x + column;
                    let y = base_y + row as u32;
                    data[(y * ATLAS_WIDTH + x) as usize] = 255;
                }
            }
        }
    }
    let (base_x, base_y) = cell_origin(WHITE_SLOT);
    for y in 0..CELL_HEIGHT {
        for x in 0..CELL_WIDTH {
            data[((base_y + y) * ATLAS_WIDTH + base_x + x) as usize] = 255;
        }
    }
    data
}

/// Atlas slot for a character (case-insensitive); unknown characters map to
/// space.
#[must_use]
pub fn slot(character: char) -> usize {
    let upper = character.to_ascii_uppercase();
    GLYPHS
        .iter()
        .position(|(candidate, _)| *candidate == upper)
        .unwrap_or(0)
}

/// UV rectangle of a glyph cell.
#[must_use]
pub fn cell_uv(slot: usize) -> ([f32; 2], [f32; 2]) {
    let (base_x, base_y) = cell_origin(slot);
    let width = ATLAS_WIDTH as f32;
    let height = ATLAS_HEIGHT as f32;
    (
        [base_x as f32 / width, base_y as f32 / height],
        [
            (base_x + CELL_WIDTH) as f32 / width,
            (base_y + CELL_HEIGHT) as f32 / height,
        ],
    )
}

/// UV of the middle of the solid-white cell.
#[must_use]
pub fn white_uv() -> [f32; 2] {
    let (base_x, base_y) = cell_origin(WHITE_SLOT);
    [
        (base_x as f32 + CELL_WIDTH as f32 / 2.0) / ATLAS_WIDTH as f32,
        (base_y as f32 + CELL_HEIGHT as f32 / 2.0) / ATLAS_HEIGHT as f32,
    ]
}

fn cell_origin(slot: usize) -> (u32, u32) {
    let slot = slot as u32;
    (
        (slot % ATLAS_COLUMNS) * CELL_WIDTH,
        (slot / ATLAS_COLUMNS) * CELL_HEIGHT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_fit_before_the_white_cell() {
        assert!(GLYPHS.len() < WHITE_SLOT);
        assert!(WHITE_SLOT < (ATLAS_COLUMNS * (ATLAS_HEIGHT / CELL_HEIGHT)) as usize);
    }

    #[test]
    fn atlas_has_expected_size_and_white_cell() {
        let data = atlas();
        assert_eq!(data.len(), (ATLAS_WIDTH * ATLAS_HEIGHT) as usize);
        let (x, y) = cell_origin(WHITE_SLOT);
        assert_eq!(data[(y * ATLAS_WIDTH + x) as usize], 255);
        assert_eq!(
            data[((y + CELL_HEIGHT - 1) * ATLAS_WIDTH + x + CELL_WIDTH - 1) as usize],
            255
        );
    }

    #[test]
    fn lookup_is_case_insensitive_and_safe() {
        assert_eq!(slot('a'), slot('A'));
        assert_ne!(slot('A'), slot('B'));
        assert_eq!(slot('\u{2603}'), slot(' '));
    }
}
