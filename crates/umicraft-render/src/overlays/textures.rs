// SPDX-License-Identifier: MIT OR Apache-2.0

//! The original stand-ins for the three pictures of the overlays.
//!
//! `misc/underwater.png`, `misc/pumpkinblur.png` and `misc/vignette.png` come
//! from the user's jar at runtime (spec §3). When one is missing the pass draws
//! these generated pictures instead: a wavy blue veil, a dark frame and a
//! radial shade.

use std::f32::consts::TAU;

use crate::sky::SkyImage;

/// Edge of every fallback picture, in texels.
const SIZE: u32 = 64;

/// Builds an image by calling `texel(u, v)` with the centre of each texel in
/// `0.0..1.0`.
fn generate(texel: impl Fn(f32, f32) -> [u8; 4]) -> SkyImage {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (
                (x as f32 + 0.5) / SIZE as f32,
                (y as f32 + 0.5) / SIZE as f32,
            );
            rgba.extend_from_slice(&texel(u, v));
        }
    }
    SkyImage {
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// Hermite step from 0 at `low` to 1 at `high`.
fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A blue veil whose opacity ripples; two sine waves make it tile because
/// both complete whole periods over the image.
pub(super) fn fallback_underwater() -> SkyImage {
    generate(|u, v| {
        let ripple = 0.5 + 0.25 * (TAU * (2.0 * u + v)).sin() + 0.25 * (TAU * (2.0 * v - u)).sin();
        [58, 104, 196, (90.0 + 120.0 * ripple) as u8]
    })
}

/// An opaque dark frame that clears towards the centre, shaped like a square
/// with rounded corners (a fourth-power norm).
pub(super) fn fallback_pumpkin() -> SkyImage {
    generate(|u, v| {
        let (dx, dy) = (u - 0.5, v - 0.5);
        let reach = (dx.powi(4) + dy.powi(4)).sqrt().sqrt();
        let alpha = smoothstep(0.34, 0.5, reach);
        [34, 18, 2, (alpha * 255.0) as u8]
    })
}

/// Black in the middle, white in the corners: the shade the vignette
/// multiplies the screen by.
pub(super) fn fallback_vignette() -> SkyImage {
    generate(|u, v| {
        let reach = (u - 0.5).hypot(v - 0.5) / std::f32::consts::FRAC_1_SQRT_2;
        let shade = (smoothstep(0.45, 1.0, reach) * 255.0) as u8;
        [shade, shade, shade, 255]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texel(image: &SkyImage, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        [
            image.rgba[at],
            image.rgba[at + 1],
            image.rgba[at + 2],
            image.rgba[at + 3],
        ]
    }

    #[test]
    fn the_fallbacks_are_valid_images() {
        for image in [
            fallback_underwater(),
            fallback_pumpkin(),
            fallback_vignette(),
        ] {
            assert!(image.is_valid());
        }
    }

    #[test]
    fn the_underwater_veil_is_translucent_everywhere() {
        let veil = fallback_underwater();
        assert!(veil
            .rgba
            .chunks_exact(4)
            .all(|p| p[3] > 60 && p[3] < 255 && p[2] > p[0]));
    }

    #[test]
    fn the_pumpkin_is_clear_in_the_middle_and_solid_at_the_corner() {
        let pumpkin = fallback_pumpkin();
        assert_eq!(texel(&pumpkin, 32, 32)[3], 0);
        assert!(texel(&pumpkin, 0, 0)[3] > 250);
    }

    #[test]
    fn the_vignette_is_dark_in_the_middle_and_bright_in_the_corner() {
        let vignette = fallback_vignette();
        assert_eq!(texel(&vignette, 32, 32)[0], 0);
        assert!(texel(&vignette, 0, 0)[0] > 250);
    }
}
