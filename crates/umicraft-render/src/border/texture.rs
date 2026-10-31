// SPDX-License-Identifier: MIT OR Apache-2.0

//! The original stand-in for `forcefield.png`.
//!
//! The real picture is read from the user's jar at runtime and never stored
//! here (spec §3). Until it arrives, or when it is missing or malformed, the
//! wall samples this generated 64 x 64 pattern: a soft white grid of bands
//! over a faint haze, tileable in both directions.

use crate::sky::SkyImage;

/// Edge of the pattern in texels.
const SIZE: u32 = 64;
/// Distance between grid lines, in texels.
const CELL: u32 = 16;

/// A deterministic, tileable soft grid.
pub(super) fn fallback_forcefield() -> SkyImage {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let band = line(x).max(line(y));
            let alpha = (40.0 + 215.0 * band) as u8;
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    SkyImage {
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// How strongly texel `t` lies on a grid line: 1 on the line, fading out
/// over three texels, wrapping at the cell size.
fn line(t: u32) -> f32 {
    let offset = t % CELL;
    let distance = offset.min(CELL - offset);
    (1.0 - distance as f32 / 3.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_is_a_valid_tileable_image() {
        let image = fallback_forcefield();
        assert!(image.is_valid());
        let alpha = |x: u32, y: u32| image.rgba[((y * SIZE + x) * 4 + 3) as usize];
        assert_eq!(line(0), line(SIZE), "the period divides the image");
        assert!(alpha(0, 8) > alpha(8, 8), "lines are brighter than cells");
    }
}
