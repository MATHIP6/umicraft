// SPDX-License-Identifier: MIT OR Apache-2.0

//! The palette of the color settings: a few presets to click.

use crate::draw::Color;

/// The presets, as `#rrggbb`: white, light grey, red, orange, yellow, green,
/// cyan and a purple, enough for text readable on most worlds.
pub(super) const PRESETS: [&str; 8] = [
    "#ffffff", "#aaaaaa", "#ff5555", "#ffaa00", "#ffff55", "#55ff55", "#55ffff", "#aa77ff",
];

/// Reads `#rrggbb` as an opaque color; `None` when it is not one.
pub(super) fn parse_hex(hex: &str) -> Option<Color> {
    let digits = hex
        .strip_prefix('#')
        .filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let value = u32::from_str_radix(digits, 16).ok()?;
    let channel = |shift: u32| ((value >> shift) & 0xff) as f32 / 255.0;
    Some([channel(16), channel(8), channel(0), 1.0])
}

/// Whether two `#rrggbb` texts are the same color, ignoring case.
pub(super) fn same_color(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hex_color_is_read_and_a_bad_one_is_refused() {
        assert_eq!(parse_hex("#ff8000"), Some([1.0, 128.0 / 255.0, 0.0, 1.0]));
        for bad in ["ff8000", "#ff80", "#gg0000", "#ff80000", "", "#+f0000"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn every_preset_is_a_valid_color() {
        assert!(PRESETS.iter().all(|p| parse_hex(p).is_some()));
    }

    #[test]
    fn colors_compare_without_regard_to_case() {
        assert!(same_color("#FFAA00", "#ffaa00"));
        assert!(!same_color("#ffaa00", "#ffaa01"));
    }
}
