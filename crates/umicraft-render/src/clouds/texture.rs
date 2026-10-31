// SPDX-License-Identifier: MIT OR Apache-2.0

//! The original stand-in for `clouds.png`.
//!
//! The real picture is read from the user's jar at runtime and never stored
//! here (spec §3). Until it arrives, or when it is missing or malformed, the
//! pass samples this generated 256 x 256 mask: tileable value noise
//! thresholded to opaque white cloud cells and fully transparent gaps, which
//! is all the geometry looks at.

use crate::sky::{hash, SkyImage};

/// Edge of the mask in cells; the reference texture has the same size.
const SIZE: u32 = 256;
/// Lattice spacing of the coarse octave, in cells.
const COARSE: u32 = 32;
/// Lattice spacing of the fine octave, in cells.
const FINE: u32 = 8;
/// Noise level above which a cell holds cloud (about 40 % coverage).
const THRESHOLD: f32 = 0.56;

/// A deterministic, tileable cloud mask.
pub(super) fn fallback_clouds() -> SkyImage {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let level = 0.7 * noise(x, y, COARSE, 0) + 0.3 * noise(x, y, FINE, 1);
            let alpha = if level > THRESHOLD { 255 } else { 0 };
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    SkyImage {
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// Smoothly interpolated lattice noise in `0.0..=1.0`; the lattice wraps at
/// the image edge so the mask tiles.
fn noise(x: u32, y: u32, spacing: u32, seed: u32) -> f32 {
    let cells = SIZE / spacing;
    let (gx, gy) = (x / spacing, y / spacing);
    let (fx, fy) = (
        smooth((x % spacing) as f32 / spacing as f32),
        smooth((y % spacing) as f32 / spacing as f32),
    );
    let at = |cx: u32, cy: u32| {
        f32::from(hash(
            (cx % cells).wrapping_add(seed << 12),
            (cy % cells) + seed,
        )) / 255.0
    };
    let top = at(gx, gy) * (1.0 - fx) + at(gx + 1, gy) * fx;
    let bottom = at(gx, gy + 1) * (1.0 - fx) + at(gx + 1, gy + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// The cubic ease of a lattice interpolation weight.
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(image: &SkyImage, x: u32, y: u32) -> u8 {
        image.rgba[((y * image.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn the_fallback_is_a_valid_deterministic_image() {
        let mask = fallback_clouds();
        assert!(mask.is_valid());
        assert_eq!((mask.width, mask.height), (256, 256));
        assert_eq!(mask, fallback_clouds());
    }

    #[test]
    fn the_mask_is_binary_with_a_sensible_coverage() {
        let mask = fallback_clouds();
        let alphas: Vec<u8> = mask.rgba.chunks_exact(4).map(|p| p[3]).collect();
        assert!(alphas.iter().all(|&a| a == 0 || a == 255));
        let covered = alphas.iter().filter(|&&a| a == 255).count() as f32 / alphas.len() as f32;
        assert!((0.2..0.6).contains(&covered), "coverage {covered}");
    }

    #[test]
    fn the_mask_tiles_across_its_edges() {
        // The lattice wraps, so a texel next to the seam is not a different
        // field: neighbouring texels across the edge agree as often as
        // interior neighbours do.
        let mask = fallback_clouds();
        let seam = (0..256)
            .filter(|&y| alpha(&mask, 255, y) == alpha(&mask, 0, y))
            .count();
        assert!(seam > 200, "{seam}");
    }
}
