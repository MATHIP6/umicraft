// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vanilla auto GUI scale factor.

/// Minimum scaled-width the vanilla algorithm enforces.
const MIN_SCALED_WIDTH: u32 = 320;
/// Minimum scaled-height the vanilla algorithm enforces.
const MIN_SCALED_HEIGHT: u32 = 240;

/// Computes the vanilla auto GUI scale factor for a physical viewport.
///
/// Replicates the `ScaledResolution` constructor with `guiScale == 0` (auto):
/// the scale starts at 1 and increments while the scaled dimensions stay
/// above 320×240.
#[must_use]
pub fn gui_scale(width: u32, height: u32) -> u32 {
    let mut scale: u32 = 1;
    loop {
        let next = scale + 1;
        if width / next >= MIN_SCALED_WIDTH && height / next >= MIN_SCALED_HEIGHT {
            scale = next;
        } else {
            break;
        }
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_scale_matches_vanilla_scaled_resolution() {
        // 640×480 → scale 2 (640/2=320≥320, 480/2=240≥240; 640/3<320 → stop)
        assert_eq!(gui_scale(640, 480), 2);
        assert_eq!(gui_scale(1280, 720), 3);
        assert_eq!(gui_scale(1920, 1080), 4);
        assert_eq!(gui_scale(400, 300), 1);
    }
}
