// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block/item texture array payload shared with the renderer.
//!
//! Every layer is a square RGBA8 image. 1.8 block and item textures are 16×16,
//! but the type keeps the edge length explicit so a future resource pack can
//! use a different size. The layout (and therefore the layer order) is the
//! deterministic [`AtlasLayout`] from `umicraft-model`.

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
    /// # Errors
    /// [`crate::AssetsError::Image`] when the dimensions or pixel length do not
    /// match the layer count.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        layout: AtlasLayout,
    ) -> crate::AssetsResult<Self> {
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
