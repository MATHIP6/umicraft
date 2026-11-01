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
}
