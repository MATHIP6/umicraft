// SPDX-License-Identifier: MIT OR Apache-2.0

//! Combined block-light and sky-light samples.
//!
//! The 1.8 wire stores two independent 4-bit values per block. Keeping them
//! separate lets the renderer scale sky light by the time of day while block
//! light (torches, glowstone, lava) stays constant.

/// A block-light and sky-light pair, each in `0..=15`.
///
/// Light values are clamped on construction so malformed or synthetic data
/// cannot produce a value the packed vertex format cannot represent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LightLevel {
    /// Light emitted or propagated by blocks.
    pub block: u8,
    /// Sky light, attenuated by blocks and scaled by the time of day in the
    /// renderer.
    pub sky: u8,
}

impl LightLevel {
    /// No block light and no sky light (caves, night without a light source).
    pub const DARK: Self = Self { block: 0, sky: 0 };

    /// Full sky light and no block light (open ground at noon).
    pub const FULL_SKY: Self = Self { block: 0, sky: 15 };

    /// Builds a sample, clamping both components to `0..=15`.
    #[must_use]
    pub const fn new(block: u8, sky: u8) -> Self {
        Self {
            block: clamp_light(block),
            sky: clamp_light(sky),
        }
    }

    /// The brighter of the two channels.
    ///
    /// Useful for coarse checks (for example fog darkness); shading keeps the
    /// channels separate so night and torch light differ.
    #[must_use]
    pub const fn combined(self) -> u8 {
        if self.block > self.sky {
            self.block
        } else {
            self.sky
        }
    }
}

/// Clamps one nibble without relying on non-const `Ord` helpers.
const fn clamp_light(level: u8) -> u8 {
    if level > 15 {
        15
    } else {
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_clamps_both_channels() {
        assert_eq!(LightLevel::new(200, 99), LightLevel { block: 15, sky: 15 });
        assert_eq!(LightLevel::new(3, 7), LightLevel { block: 3, sky: 7 });
    }

    #[test]
    fn combined_is_the_brightest_channel() {
        assert_eq!(LightLevel::new(4, 12).combined(), 12);
        assert_eq!(LightLevel::new(15, 2).combined(), 15);
        assert_eq!(LightLevel::DARK.combined(), 0);
    }
}
