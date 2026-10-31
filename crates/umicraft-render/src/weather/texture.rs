// SPDX-License-Identifier: MIT OR Apache-2.0

//! The original stand-ins for `rain.png` and `snow.png`.
//!
//! The real pictures are read from the user's jar at runtime and never stored
//! here (spec §3). Until they arrive, or when one is missing or malformed, the
//! pass samples these generated pictures: pale blue streaks for the rain and
//! soft white flakes for the snow, on transparent ground.

use crate::sky::{hash, SkyImage};

/// Width of the rain picture; it spans one block.
const RAIN_WIDTH: u32 = 32;
/// Height of the rain picture; it spans four blocks (`v = y * 0.25`).
const RAIN_HEIGHT: u32 = 128;
/// Streak columns of the rain picture.
const STREAKS: [u32; 5] = [3, 9, 15, 21, 27];
/// Edge of the snow picture.
const SNOW_SIZE: u32 = 32;
/// Flake centres of the snow picture.
const FLAKES: [(u32, u32); 6] = [(5, 6), (20, 3), (12, 17), (27, 14), (4, 27), (22, 26)];

/// Pale blue vertical streaks of different lengths, fading towards the tail.
pub(super) fn fallback_rain() -> SkyImage {
    let mut rgba = vec![0u8; (RAIN_WIDTH * RAIN_HEIGHT * 4) as usize];
    for (index, &x) in STREAKS.iter().enumerate() {
        let top = u32::from(hash(index as u32, 7)) % (RAIN_HEIGHT - 40);
        let length = 24 + u32::from(hash(index as u32, 11)) % 16;
        for step in 0..length {
            let fade = 1.0 - step as f32 / length as f32;
            let at = (((top + step) * RAIN_WIDTH + x) * 4) as usize;
            rgba[at..at + 4].copy_from_slice(&[170, 190, 235, (40.0 + 180.0 * fade) as u8]);
        }
    }
    SkyImage {
        width: RAIN_WIDTH,
        height: RAIN_HEIGHT,
        rgba,
    }
}

/// Small white flakes, brightest at their centre.
pub(super) fn fallback_snow() -> SkyImage {
    let mut rgba = vec![0u8; (SNOW_SIZE * SNOW_SIZE * 4) as usize];
    for &(cx, cy) in &FLAKES {
        for (dx, dy, alpha) in [
            (0, 0, 255),
            (1, 0, 150),
            (-1, 0, 150),
            (0, 1, 150),
            (0, -1, 150),
        ] {
            let (x, y) = (
                cx.wrapping_add_signed(dx) % SNOW_SIZE,
                cy.wrapping_add_signed(dy) % SNOW_SIZE,
            );
            let at = ((y * SNOW_SIZE + x) * 4) as usize;
            rgba[at..at + 4].copy_from_slice(&[255, 255, 255, alpha]);
        }
    }
    SkyImage {
        width: SNOW_SIZE,
        height: SNOW_SIZE,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque_texels(image: &SkyImage) -> usize {
        image.rgba.chunks_exact(4).filter(|p| p[3] > 0).count()
    }

    #[test]
    fn the_fallbacks_are_valid_deterministic_and_mostly_transparent() {
        for image in [fallback_rain(), fallback_snow()] {
            assert!(image.is_valid());
            let total = (image.width * image.height) as usize;
            let covered = opaque_texels(&image);
            assert!(covered > 0 && covered < total / 4, "{covered} of {total}");
        }
        assert_eq!(fallback_rain(), fallback_rain());
    }

    #[test]
    fn the_rain_streaks_stay_inside_the_picture() {
        let image = fallback_rain();
        assert_eq!(image.rgba.len(), (RAIN_WIDTH * RAIN_HEIGHT * 4) as usize);
        assert!(opaque_texels(&image) > 100);
    }
}
