// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vanilla GUI scale factor: automatic, or capped by the "GUI Scale"
//! video option.

use std::sync::atomic::{AtomicU32, Ordering};

/// Minimum scaled-width the vanilla algorithm enforces.
const MIN_SCALED_WIDTH: u32 = 320;
/// Minimum scaled-height the vanilla algorithm enforces.
const MIN_SCALED_HEIGHT: u32 = 240;

/// The result of vanilla's `ScaledResolution` constructor: the integer GUI
/// scale factor and the viewport size in GUI units.
///
/// Source: `net.minecraft.client.gui.ScaledResolution` of MCP 1.8 (ADR-0001).
/// The unicode-font parity rule of the original (odd scales are lowered while
/// the unicode font is on) is not reproduced: Umicraft has no unicode flag.
///
/// # Example
///
/// ```
/// use umicraft_ui::screens::ScaledResolution;
///
/// let auto = ScaledResolution::new(1280, 720, 0);
/// assert_eq!(auto.scale_factor, 3);
/// assert_eq!((auto.scaled_width, auto.scaled_height), (427, 240));
/// assert_eq!(ScaledResolution::new(1280, 720, 2).scale_factor, 2);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaledResolution {
    /// Physical pixels per GUI unit, at least 1.
    pub scale_factor: u32,
    /// Viewport width in GUI units, rounded up like vanilla's `ceil`.
    pub scaled_width: u32,
    /// Viewport height in GUI units, rounded up like vanilla's `ceil`.
    pub scaled_height: u32,
}

impl ScaledResolution {
    /// Computes the scale for a physical viewport.
    ///
    /// `gui_scale_option` is the "GUI Scale" video setting: `0` is auto (the
    /// largest scale that keeps at least 320x240 GUI units), any other value
    /// caps that scale. A zero-sized viewport yields scale 1.
    #[must_use]
    pub fn new(width: u32, height: u32, gui_scale_option: u32) -> Self {
        let cap = if gui_scale_option == 0 {
            u32::MAX
        } else {
            gui_scale_option
        };
        let mut scale_factor = 1;
        while scale_factor < cap
            && width / (scale_factor + 1) >= MIN_SCALED_WIDTH
            && height / (scale_factor + 1) >= MIN_SCALED_HEIGHT
        {
            scale_factor += 1;
        }
        Self {
            scale_factor,
            scaled_width: width.div_ceil(scale_factor),
            scaled_height: height.div_ceil(scale_factor),
        }
    }
}

/// The "GUI Scale" option (`GameSettings.guiScale`): `0` auto, `1..=3` caps.
///
/// One process-wide value, because every screen and the HUD derive their
/// scale from the window size alone and none of them is handed the options.
static GUI_SCALE_OPTION: AtomicU32 = AtomicU32::new(0);

/// Sets the "GUI Scale" option that [`gui_scale`] applies: `0` is auto, any
/// other value caps the scale (vanilla offers `1`, `2` and `3`).
pub fn set_gui_scale_option(option: u32) {
    GUI_SCALE_OPTION.store(option, Ordering::Relaxed);
}

/// The "GUI Scale" option currently applied by [`gui_scale`].
#[must_use]
pub fn gui_scale_option() -> u32 {
    GUI_SCALE_OPTION.load(Ordering::Relaxed)
}

/// Computes the GUI scale factor for a physical viewport.
///
/// Equivalent to [`ScaledResolution::new`] with the current "GUI Scale"
/// option (see [`set_gui_scale_option`]): auto by default, where the scale
/// grows while the scaled dimensions stay at least 320x240.
#[must_use]
pub fn gui_scale(width: u32, height: u32) -> u32 {
    ScaledResolution::new(width, height, gui_scale_option()).scale_factor
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

    #[test]
    fn auto_scale_covers_one_to_four() {
        for (w, h, scale) in [
            (320, 240, 1),
            (640, 480, 2),
            (1280, 720, 3),
            (1920, 1080, 4),
        ] {
            assert_eq!(ScaledResolution::new(w, h, 0).scale_factor, scale);
        }
    }

    #[test]
    fn large_viewport_is_limited_by_the_height() {
        // 2160 / 9 = 240 while 2160 / 10 < 240, so 4K auto is 9, not 12.
        assert_eq!(ScaledResolution::new(3840, 2160, 0).scale_factor, 9);
    }

    #[test]
    fn option_caps_the_scale() {
        assert_eq!(ScaledResolution::new(1920, 1080, 2).scale_factor, 2);
        assert_eq!(ScaledResolution::new(1920, 1080, 1).scale_factor, 1);
        // A cap above the auto scale changes nothing.
        assert_eq!(ScaledResolution::new(640, 480, 4).scale_factor, 2);
    }

    #[test]
    fn scaled_size_uses_ceiling_division() {
        let r = ScaledResolution::new(1280, 720, 0);
        assert_eq!((r.scaled_width, r.scaled_height), (427, 240));
        let r = ScaledResolution::new(0, 0, 0);
        assert_eq!((r.scale_factor, r.scaled_width), (1, 0));
    }
}
