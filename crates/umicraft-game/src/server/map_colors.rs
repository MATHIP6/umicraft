// SPDX-License-Identifier: MIT OR Apache-2.0

//! The map colour table: a map byte is `base colour * 4 + brightness`.
//!
//! Source: MCP `MapColor` (the 36 base colours) and `MapColor.func_151643_b`
//! (the brightness multipliers 180, 220, 255 and 135 of 255), with the
//! transparent checkerboard of `MapItemRenderer.Instance.updateMapTexture`
//! for the base colour 0.

/// The RGB value of each base colour, indexed by `MapColor.colorIndex`.
const BASE_COLORS: [u32; 36] = [
    0x000000, 0x7FB238, 0xF7E9A3, 0xC7C7C7, 0xFF0000, 0xA0A0FF, 0xA7A7A7, 0x007C00, 0xFFFFFF,
    0xA4A8B8, 0x976D4D, 0x707070, 0x4040FF, 0x8F7748, 0xFFFCF5, 0xD87F33, 0xB24CD8, 0x6699D8,
    0xE5E533, 0x7FCC19, 0xF27FA5, 0x4C4C4C, 0x999999, 0x4C7F99, 0x7F3FB2, 0x334CB2, 0x664C33,
    0x667F33, 0x993333, 0x191919, 0xFAEE4D, 0x5CDBD5, 0x4A80FF, 0x00D93A, 0x815631, 0x700200,
];

/// The multiplier (out of 255) of each brightness `byte & 3`.
const BRIGHTNESS: [u32; 4] = [180, 220, 255, 135];

/// Pixels per map row, for the checkerboard of transparent pixels.
const ROW: usize = 128;

/// The RGBA pixel of colour byte `color` at pixel number `pixel` of the map.
///
/// Base colour 0 is transparent: the reference paints a faint dark
/// checkerboard (alpha 16 and 24) there. A base colour beyond the table has
/// no entry in the reference (it would throw); it is drawn transparent here
/// because the byte comes from the network.
#[must_use]
pub fn map_pixel(color: u8, pixel: usize) -> [u8; 4] {
    let base = usize::from(color / 4);
    match BASE_COLORS.get(base) {
        Some(&rgb) if base != 0 => {
            let scale = BRIGHTNESS[usize::from(color & 3)];
            let channel = |shift: u32| ((rgb >> shift & 0xFF) * scale / 255) as u8;
            [channel(16), channel(8), channel(0), 255]
        }
        Some(_) => {
            let alpha = ((pixel + pixel / ROW) & 1) * 8 + 16;
            [0, 0, 0, alpha as u8]
        }
        None => [0, 0, 0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grass_follows_the_four_brightness_multipliers() {
        // Grass 0x7FB238 = (127, 178, 56); byte = 1 * 4 + brightness.
        assert_eq!(map_pixel(4, 0), [89, 125, 39, 255]);
        assert_eq!(map_pixel(5, 0), [109, 153, 48, 255]);
        assert_eq!(map_pixel(6, 0), [127, 178, 56, 255]);
        assert_eq!(map_pixel(7, 0), [67, 94, 29, 255]);
    }

    #[test]
    fn white_snow_at_full_brightness_stays_white() {
        assert_eq!(map_pixel(8 * 4 + 2, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn the_air_colour_is_a_faint_checkerboard() {
        assert_eq!(map_pixel(0, 0), [0, 0, 0, 16]);
        assert_eq!(map_pixel(3, 1), [0, 0, 0, 24]);
        // The next row starts with the opposite phase.
        assert_eq!(map_pixel(0, 128), [0, 0, 0, 24]);
    }

    #[test]
    fn unknown_base_colours_are_transparent() {
        assert_eq!(map_pixel(255, 0), [0, 0, 0, 0]);
        assert_eq!(map_pixel(36 * 4, 0), [0, 0, 0, 0]);
        assert_eq!(map_pixel(35 * 4 + 2, 0), [112, 2, 0, 255]);
    }
}
