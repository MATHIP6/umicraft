// SPDX-License-Identifier: MIT OR Apache-2.0

//! The original stand-in for `particles.png` (spec section 3: no vanilla
//! picture in the repository).
//!
//! The real sheet is 16 x 16 cells of 8 texels, read from the user's
//! installation. The stand-in has the same grid, with a soft white disc in each
//! cell whose radius shrinks along the row, so the particles that step through
//! eight frames (smoke, spells, explosions) still fade out.

use crate::sky::SkyImage;

/// Cells per side of the sheet.
const GRID: u32 = 16;
/// Texels per cell side.
const CELL: u32 = 8;

/// A 128 x 128 sheet with a shrinking disc in every cell.
pub(super) fn fallback_sheet() -> SkyImage {
    let side = GRID * CELL;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let column = (x / CELL) % 8;
            let radius = 0.5 - 0.04 * column as f32;
            let dx = ((x % CELL) as f32 + 0.5) / CELL as f32 - 0.5;
            let dy = ((y % CELL) as f32 + 0.5) / CELL as f32 - 0.5;
            let alpha = if dx.hypot(dy) <= radius { 255 } else { 0 };
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    SkyImage {
        width: side,
        height: side,
        rgba,
    }
}

/// Edge of the fallback footprint in texels.
const FOOTPRINT_SIDE: u32 = 16;
/// Frames per side of the large explosion sheet (4 x 4, 16 frames).
const EXPLOSION_GRID: u32 = 4;
/// Texels per frame side of the fallback explosion.
const EXPLOSION_FRAME: u32 = 16;

/// A white oval standing in for `footprint.png`: a longer sole and a round
/// heel, drawn as two ellipses.
pub(super) fn fallback_footprint() -> SkyImage {
    let side = FOOTPRINT_SIDE;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let u = (x as f32 + 0.5) / side as f32 - 0.5;
            let v = (y as f32 + 0.5) / side as f32;
            let sole = (u / 0.25).powi(2) + ((v - 0.35) / 0.3).powi(2) <= 1.0;
            let heel = (u / 0.18).powi(2) + ((v - 0.8) / 0.16).powi(2) <= 1.0;
            let alpha = if sole || heel { 255 } else { 0 };
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    SkyImage {
        width: side,
        height: side,
        rgba,
    }
}

/// A 4 x 4 sheet of discs that swell frame by frame, standing in for
/// `explosion.png` (the real frames are a ball of smoke and sparks).
pub(super) fn fallback_explosion() -> SkyImage {
    let side = EXPLOSION_GRID * EXPLOSION_FRAME;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let frame = (y / EXPLOSION_FRAME) * EXPLOSION_GRID + x / EXPLOSION_FRAME;
            let radius = 0.1 + 0.4 * frame as f32 / 15.0;
            let dx = ((x % EXPLOSION_FRAME) as f32 + 0.5) / EXPLOSION_FRAME as f32 - 0.5;
            let dy = ((y % EXPLOSION_FRAME) as f32 + 0.5) / EXPLOSION_FRAME as f32 - 0.5;
            let alpha = if dx.hypot(dy) <= radius { 255 } else { 0 };
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    SkyImage {
        width: side,
        height: side,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_is_valid_and_has_a_solid_centre_and_clear_corners() {
        let sheet = fallback_sheet();
        assert!(sheet.is_valid());
        let alpha = |x: u32, y: u32| sheet.rgba[((y * sheet.width + x) * 4 + 3) as usize];
        assert_eq!(alpha(4, 4), 255);
        assert_eq!(alpha(0, 0), 0);
    }

    #[test]
    fn the_footprint_and_the_explosion_stand_ins_are_valid() {
        let footprint = fallback_footprint();
        let explosion = fallback_explosion();
        assert!(footprint.is_valid() && explosion.is_valid());
        let alpha =
            |image: &SkyImage, x: u32, y: u32| image.rgba[((y * image.width + x) * 4 + 3) as usize];
        assert_eq!(alpha(&footprint, 8, 5), 255, "the sole");
        assert_eq!(alpha(&footprint, 0, 0), 0);
        // The last frame fills more of its cell than the first.
        let covered = |column: u32, row: u32| {
            (0..EXPLOSION_FRAME)
                .flat_map(|y| (0..EXPLOSION_FRAME).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    alpha(
                        &explosion,
                        column * EXPLOSION_FRAME + x,
                        row * EXPLOSION_FRAME + y,
                    ) > 0
                })
                .count()
        };
        assert!(covered(3, 3) > covered(0, 0) * 4);
    }
}
