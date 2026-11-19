// SPDX-License-Identifier: MIT OR Apache-2.0
//! The validation rules of setting values, as pure functions.
//!
//! They are public so that settings declared at run time (external mods, ADR-0086)
//! are held to exactly the rules of the built-in ones.

/// Clamps `value` to `[min, max]` and snaps it to the nearest multiple of `step`
/// counted from `min`. `None` when `value` is not finite or `step` is not positive.
///
/// Clamping comes last, so a `max` that is not on the grid is still reachable.
#[must_use]
pub fn snap_slider(value: f32, min: f32, max: f32, step: f32) -> Option<f32> {
    if !value.is_finite() || step <= 0.0 {
        return None;
    }
    let snapped = min + ((value - min) / step).round() * step;
    Some(snapped.clamp(min, max))
}

/// Parses a `#rrggbb` colour (either case of hex digit) into `0xRRGGBB`.
#[must_use]
pub fn parse_hex_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Writes `0xRRGGBB` as lowercase `#rrggbb`; bits above the 24th are ignored.
#[must_use]
pub fn format_hex_color(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0x00ff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_values_are_clamped_and_snapped() {
        assert_eq!(snap_slider(2.3, 0.0, 10.0, 0.5), Some(2.5));
        assert_eq!(snap_slider(99.0, 0.0, 10.0, 0.5), Some(10.0));
        assert_eq!(snap_slider(-3.0, 0.0, 10.0, 0.5), Some(0.0));
    }

    #[test]
    fn a_bad_slider_input_is_none() {
        assert_eq!(snap_slider(f32::NAN, 0.0, 1.0, 0.1), None);
        assert_eq!(snap_slider(f32::INFINITY, 0.0, 1.0, 0.1), None);
        assert_eq!(snap_slider(0.5, 0.0, 1.0, 0.0), None);
    }

    #[test]
    fn hex_colours_parse_and_format_back() {
        assert_eq!(parse_hex_color("#1a2B3c"), Some(0x1a_2b_3c));
        assert_eq!(format_hex_color(0x1a_2b_3c), "#1a2b3c");
        assert_eq!(format_hex_color(0xff_00_00_00 | 0x0000ff), "#0000ff");
        for bad in ["red", "#fff", "#gggggg", "1a2b3c", "#1a2b3c0", "#+12345"] {
            assert_eq!(parse_hex_color(bad), None, "{bad}");
        }
    }
}
