// SPDX-License-Identifier: MIT OR Apache-2.0

//! The character order of the 1.8 `ascii.png` glyph grid.
//!
//! The atlas is a 16x16 grid; the character at index `n` of [`GRID_CHARS`]
//! sits in cell `(n % 16, n / 16)`. The order is the `charList` string that
//! the reference `FontRenderer` indexes (`FontRenderer.java:237,455`, consulted
//! under ADR-0001); it is a character table, not an asset. It covers Latin-1
//! accents, Turkish and Polish letters, box-drawing characters and a few Greek
//! and maths symbols. `'\0'` marks the unused cells.

/// Number of cells of the grid.
pub const GRID_CELLS: usize = 256;

/// The character of each grid cell, in cell order (`'\0'` = unused).
pub const GRID_CHARS: &str = "\u{00c0}\u{00c1}\u{00c2}\u{00c8}\u{00ca}\u{00cb}\u{00cd}\u{00d3}\u{00d4}\u{00d5}\u{00da}\u{00df}\u{00e3}\u{00f5}\u{011f}\u{0130}\u{0131}\u{0152}\u{0153}\u{015e}\u{015f}\u{0174}\u{0175}\u{017e}\u{0207}\u{0000}\u{0000}\u{0000}\u{0000}\u{0000}\u{0000}\u{0000} !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\u{0000}\u{00c7}\u{00fc}\u{00e9}\u{00e2}\u{00e4}\u{00e0}\u{00e5}\u{00e7}\u{00ea}\u{00eb}\u{00e8}\u{00ef}\u{00ee}\u{00ec}\u{00c4}\u{00c5}\u{00c9}\u{00e6}\u{00c6}\u{00f4}\u{00f6}\u{00f2}\u{00fb}\u{00f9}\u{00ff}\u{00d6}\u{00dc}\u{00f8}\u{00a3}\u{00d8}\u{00d7}\u{0192}\u{00e1}\u{00ed}\u{00f3}\u{00fa}\u{00f1}\u{00d1}\u{00aa}\u{00ba}\u{00bf}\u{00ae}\u{00ac}\u{00bd}\u{00bc}\u{00a1}\u{00ab}\u{00bb}\u{2591}\u{2592}\u{2593}\u{2502}\u{2524}\u{2561}\u{2562}\u{2556}\u{2555}\u{2563}\u{2551}\u{2557}\u{255d}\u{255c}\u{255b}\u{2510}\u{2514}\u{2534}\u{252c}\u{251c}\u{2500}\u{253c}\u{255e}\u{255f}\u{255a}\u{2554}\u{2569}\u{2566}\u{2560}\u{2550}\u{256c}\u{2567}\u{2568}\u{2564}\u{2565}\u{2559}\u{2558}\u{2552}\u{2553}\u{256b}\u{256a}\u{2518}\u{250c}\u{2588}\u{2584}\u{258c}\u{2590}\u{2580}\u{03b1}\u{03b2}\u{0393}\u{03c0}\u{03a3}\u{03c3}\u{03bc}\u{03c4}\u{03a6}\u{0398}\u{03a9}\u{03b4}\u{221e}\u{2205}\u{2208}\u{2229}\u{2261}\u{00b1}\u{2265}\u{2264}\u{2320}\u{2321}\u{00f7}\u{2248}\u{00b0}\u{2219}\u{00b7}\u{221a}\u{207f}\u{00b2}\u{25a0}\u{0000}";

#[cfg(test)]
mod tests {
    use super::{GRID_CELLS, GRID_CHARS};

    #[test]
    fn the_table_fills_the_whole_grid() {
        assert_eq!(GRID_CHARS.chars().count(), GRID_CELLS);
    }

    #[test]
    fn ascii_keeps_its_code_point_as_slot() {
        let chars: Vec<char> = GRID_CHARS.chars().collect();
        for code in 32..=126usize {
            assert_eq!(chars[code] as usize, code);
        }
    }

    #[test]
    fn accents_sit_where_the_reference_puts_them() {
        let chars: Vec<char> = GRID_CHARS.chars().collect();
        assert_eq!(chars[0], '\u{c0}');
        assert_eq!(chars[130], '\u{e9}');
        assert_eq!(chars[164], '\u{f1}');
    }
}
