// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 grass and foliage colour maps (`ColorizerGrass`,
//! `ColorizerFoliage`).
//!
//! Both maps are 256×256 RGB images inside the verified client jar at
//! `assets/minecraft/textures/colormap/{grass,foliage}.png`. They are Mojang
//! assets: this module only decodes them from the user's jar at runtime and
//! never writes them to disk or into a release (spec §3, ADR-0001).
//!
//! The maps are indexed by the biome climate: the x axis is `1 - temperature`
//! and the y axis is `1 - temperature * rainfall`, both scaled to `0..=255`.
//! [`sample`](Colormap::sample) reproduces the 1.8 lookup exactly so the
//! generated tint matches the vanilla client.
//!
//! ```text
//! temperature ──▶ x = (1 - temp) * 255
//! rainfall ─────▶ y = (1 - temp * rain) * 255
//! colour = pixels[y * 256 + x]
//! ```

use crate::error::{image_error, AssetsResult};

/// Width and height of a 1.8 colormap in pixels.
pub const SIZE: u32 = 256;

/// Virtual jar prefix of the colormaps.
pub(crate) const COLORMAP_PREFIX: &str = "assets/minecraft/textures/colormap/";

/// Virtual jar path of the grass colormap.
pub(crate) const GRASS_PATH: &str = "assets/minecraft/textures/colormap/grass.png";

/// Virtual jar path of the foliage colormap.
pub(crate) const FOLIAGE_PATH: &str = "assets/minecraft/textures/colormap/foliage.png";

/// A decoded 256×256 biome colour map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colormap {
    /// Row-major RGB triples, `SIZE * SIZE` entries.
    pixels: Vec<[u8; 3]>,
}

impl Colormap {
    /// Builds a colormap from tightly packed RGBA8 pixels.
    ///
    /// `path` is only used in the error message. The alpha channel is ignored,
    /// like in the vanilla texture loader.
    ///
    /// # Errors
    ///
    /// [`crate::AssetsError::Image`] when the image is not exactly `SIZE`×`SIZE` or
    /// when `pixels` does not contain `SIZE * SIZE` RGBA texels.
    pub fn from_rgba8(path: &str, width: u32, height: u32, pixels: &[u8]) -> AssetsResult<Self> {
        if width != SIZE || height != SIZE {
            return Err(image_error(
                path,
                format!("expected a {SIZE}x{SIZE} colormap, got {width}x{height}"),
            ));
        }
        let texels = (SIZE * SIZE) as usize;
        if pixels.len() != texels * 4 {
            return Err(image_error(
                path,
                format!(
                    "expected {} RGBA bytes for a {SIZE}x{SIZE} colormap, got {}",
                    texels * 4,
                    pixels.len()
                ),
            ));
        }
        let rgb = pixels
            .chunks_exact(4)
            .map(|texel| [texel[0], texel[1], texel[2]])
            .collect();
        Ok(Self { pixels: rgb })
    }

    /// Samples the vanilla colour at a biome climate.
    ///
    /// `temperature` and `rainfall` are clamped to `0..=1` first, then the
    /// 1.8 `ColorizerGrass` lookup is applied: `rainfall *= temperature`,
    /// `x = (1 - temperature) * 255`, `y = (1 - rainfall) * 255`, index
    /// `y * 256 + x` (integer truncation, like the `(int)` casts).
    #[must_use]
    pub fn sample(&self, temperature: f32, rainfall: f32) -> [u8; 3] {
        let temperature = temperature.clamp(0.0, 1.0);
        let rainfall = (rainfall * temperature).clamp(0.0, 1.0);
        let x = ((1.0 - temperature) * 255.0) as usize;
        let y = ((1.0 - rainfall) * 255.0) as usize;
        self.pixels[y * SIZE as usize + x]
    }
}

/// The two colormaps a complete 1.8 installation ships.
#[derive(Debug, Clone)]
pub struct BiomeColormaps {
    /// Grass block, tall grass and double-plant colours.
    pub grass: Colormap,
    /// Leaves and vine colours.
    pub foliage: Colormap,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AssetsError;

    /// Builds a map whose pixel `(x, y)` is `[x, y, 7]`, so a sample proves
    /// which coordinates were looked up.
    fn coordinate_map() -> Colormap {
        let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                pixels.extend_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        Colormap::from_rgba8("test", SIZE, SIZE, &pixels).expect("the synthetic map decodes")
    }

    #[test]
    fn sampling_hits_the_four_corners_and_the_middle() {
        let map = coordinate_map();
        assert_eq!(map.sample(1.0, 1.0), [0, 0, 7], "top-left");
        assert_eq!(map.sample(1.0, 0.0), [0, 255, 7], "bottom-left");
        assert_eq!(map.sample(0.0, 0.0), [255, 255, 7], "bottom-right");
        assert_eq!(map.sample(0.0, 1.0), [255, 255, 7], "rain scaled by 0");
        assert_eq!(map.sample(0.5, 0.5), [127, 191, 7], "middle");
    }

    #[test]
    fn sampling_clamps_out_of_range_climates() {
        let map = coordinate_map();
        assert_eq!(map.sample(2.0, 2.0), [0, 0, 7]);
        assert_eq!(map.sample(-1.0, -1.0), [255, 255, 7]);
    }

    #[test]
    fn a_wrong_size_or_truncated_map_is_rejected() {
        let wrong_edge = Colormap::from_rgba8("test", 128, 128, &vec![0; 128 * 128 * 4]);
        assert!(matches!(wrong_edge, Err(AssetsError::Image { .. })));

        let short = Colormap::from_rgba8("test", SIZE, SIZE, &[0; 16]);
        let Err(AssetsError::Image { reason, .. }) = short else {
            panic!("a truncated colormap must be rejected");
        };
        assert!(reason.contains("RGBA bytes"), "reason: {reason}");
    }
}
