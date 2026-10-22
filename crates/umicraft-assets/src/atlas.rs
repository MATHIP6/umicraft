// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block/item texture array payload shared with the renderer.
//!
//! Every layer is an RGBA8 image of the same size. The builder normalises each
//! texture to its first square frame and scales it to the largest frame edge in
//! the source (32×32 for the 1.8.8 jar), but the type keeps the dimensions
//! explicit so a resource pack can use a different size. The layout (and
//! therefore the layer order) is the deterministic [`AtlasLayout`] from
//! `umicraft-model`; layer zero is the original missing-texture checker.

use umicraft_model::AtlasLayout;

/// One texture-array upload: layer-major RGBA8 pixels plus their key layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    layout: AtlasLayout,
}

impl BlockAtlas {
    /// Builds an atlas from already decoded layers in layout order.
    ///
    /// `pixels` must be exactly `width * height * 4 * layout.len()` bytes
    /// long. The first layer is expected to be the missing-texture checker,
    /// though this constructor only validates the arithmetic.
    ///
    /// # Errors
    /// [`crate::AssetsError::Image`] when a dimension is zero, the layout is
    /// empty, the dimensions or pixel length do not match the layer count, or
    /// the size arithmetic overflows.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        layout: AtlasLayout,
    ) -> crate::AssetsResult<Self> {
        if width == 0 || height == 0 {
            return Err(crate::AssetsError::Image {
                path: "atlas".to_string(),
                reason: format!("the layer size {width}x{height} is empty"),
            });
        }
        if layout.is_empty() {
            return Err(crate::AssetsError::Image {
                path: "atlas".to_string(),
                reason: "the layout has no layers".to_string(),
            });
        }
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| bytes.checked_mul(u64::from(layout.len())))
            .ok_or_else(|| crate::AssetsError::Image {
                path: "atlas".to_string(),
                reason: "the texture array size overflows".to_string(),
            })?;
        if pixels.len() as u64 != expected {
            return Err(crate::AssetsError::Image {
                path: "atlas".to_string(),
                reason: format!(
                    "{} layers of {width}x{height} need {expected} bytes, got {}",
                    layout.len(),
                    pixels.len()
                ),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
            layout,
        })
    }

    /// Edge length of one layer in texels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of one layer in texels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Number of layers.
    #[must_use]
    pub fn layers(&self) -> u32 {
        self.layout.len()
    }

    /// Layer-major tightly packed RGBA8 pixels.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Texture-key to layer mapping.
    #[must_use]
    pub fn layout(&self) -> &AtlasLayout {
        &self.layout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_layer_two_by_two_atlas_has_the_expected_length() {
        let layout = AtlasLayout::from_keys(["blocks/stone"]);
        let atlas = BlockAtlas::new(2, 2, vec![7; 2 * 2 * 4 * 2], layout)
            .expect("a matching buffer is accepted");
        assert_eq!(atlas.width(), 2);
        assert_eq!(atlas.height(), 2);
        assert_eq!(atlas.layers(), 2);
        assert_eq!(atlas.pixels().len(), 32);
    }

    #[test]
    fn an_empty_buffer_or_zero_size_is_rejected() {
        let layout = AtlasLayout::from_keys(["blocks/stone"]);
        let err = BlockAtlas::new(2, 2, Vec::new(), layout.clone())
            .expect_err("a short buffer must fail");
        assert!(matches!(err, crate::AssetsError::Image { .. }));

        let err =
            BlockAtlas::new(0, 2, Vec::new(), layout).expect_err("a zero dimension must fail");
        assert!(matches!(err, crate::AssetsError::Image { .. }));
    }

    #[test]
    fn an_empty_layout_is_rejected() {
        let err = BlockAtlas::new(2, 2, Vec::new(), AtlasLayout::default())
            .expect_err("a layout without the missing layer must fail");
        assert!(matches!(err, crate::AssetsError::Image { .. }));
    }
}
