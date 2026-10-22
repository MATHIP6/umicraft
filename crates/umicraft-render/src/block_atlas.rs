// SPDX-License-Identifier: MIT OR Apache-2.0

//! Validation for the block/item texture array built from the user's
//! installation.
//!
//! `umicraft-assets` builds the atlas from the verified client jar and hands it
//! to the renderer as plain primitives (layer size, layer count, layer-major
//! RGBA8 pixels), so the renderer needs no dependency on the asset crate.
//! `wgpu` panics on an invalid texture descriptor, and sampling past the
//! uploaded layers is undefined, so every upload is validated here first and a
//! malformed atlas is rejected with a typed [`BlockAtlasError`] before any GPU
//! resource is created.

use thiserror::Error;

/// Reasons a block/item texture array cannot be uploaded.
///
/// Every variant is returned by [`crate::Renderer::set_block_atlas`] before it
/// touches the GPU, so a rejected upload always leaves the previous array (the
/// procedural fallback at startup) bound and usable.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BlockAtlasError {
    /// One of the layer width, height or count is zero.
    #[error("block atlas must have a non-zero width, height and layer count")]
    EmptySize,
    /// `width × height × 4 × layers` does not fit in a byte count.
    #[error("block atlas {width}x{height} with {layers} layers overflows a byte count")]
    SizeOverflow {
        /// Declared layer width in texels.
        width: u32,
        /// Declared layer height in texels.
        height: u32,
        /// Declared layer count.
        layers: u32,
    },
    /// The pixel buffer is not `width × height × 4 × layers` bytes long.
    #[error(
        "block atlas has {actual} bytes, expected {expected} for {width}x{height} with {layers} layers"
    )]
    PixelCount {
        /// Declared layer width in texels.
        width: u32,
        /// Declared layer height in texels.
        height: u32,
        /// Declared layer count.
        layers: u32,
        /// `width × height × 4 × layers`, the required buffer length.
        expected: usize,
        /// Length of the supplied pixel buffer.
        actual: usize,
    },
    /// A layer is wider or taller than the device's 2D texture limit.
    #[error("block atlas layer {width}x{height} exceeds the device limit of {max} texels")]
    TooLarge {
        /// Declared layer width in texels.
        width: u32,
        /// Declared layer height in texels.
        height: u32,
        /// `wgpu::Limits::max_texture_dimension_2d` of the device.
        max: u32,
    },
    /// The array has more layers than the device supports.
    #[error("block atlas has {layers} layers, exceeding the device limit of {max}")]
    TooManyLayers {
        /// Declared layer count.
        layers: u32,
        /// `wgpu::Limits::max_texture_array_layers` of the device.
        max: u32,
    },
}

/// Checks one proposed array against the device limits.
///
/// `pixel_bytes` is the length of the layer-major RGBA8 buffer;
/// `max_texture_dimension_2d` and `max_texture_array_layers` are the matching
/// `wgpu::Limits` of the device that would receive the upload.
///
/// # Errors
///
/// Returns the matching [`BlockAtlasError`] when a dimension or the layer
/// count is zero, the size arithmetic overflows, the buffer length does not
/// match, a layer exceeds the device's 2D texture limit, or the array has more
/// layers than the device supports.
pub(crate) fn validate(
    width: u32,
    height: u32,
    layers: u32,
    pixel_bytes: usize,
    max_texture_dimension_2d: u32,
    max_texture_array_layers: u32,
) -> Result<(), BlockAtlasError> {
    if width == 0 || height == 0 || layers == 0 {
        return Err(BlockAtlasError::EmptySize);
    }
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|texels| texels.checked_mul(4))
        .and_then(|bytes| bytes.checked_mul(u64::from(layers)))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(BlockAtlasError::SizeOverflow {
            width,
            height,
            layers,
        })?;
    if pixel_bytes != expected {
        return Err(BlockAtlasError::PixelCount {
            width,
            height,
            layers,
            expected,
            actual: pixel_bytes,
        });
    }
    if width > max_texture_dimension_2d || height > max_texture_dimension_2d {
        return Err(BlockAtlasError::TooLarge {
            width,
            height,
            max: max_texture_dimension_2d,
        });
    }
    if layers > max_texture_array_layers {
        return Err(BlockAtlasError::TooManyLayers {
            layers,
            max: max_texture_array_layers,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_DIM: u32 = 8192;
    const MAX_LAYERS: u32 = 256;

    #[test]
    fn a_matching_buffer_passes() {
        assert_eq!(
            validate(2, 2, 3, 2 * 2 * 4 * 3, MAX_DIM, MAX_LAYERS),
            Ok(())
        );
    }

    #[test]
    fn zero_dimensions_and_layers_are_rejected() {
        assert_eq!(
            validate(0, 2, 1, 0, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::EmptySize)
        );
        assert_eq!(
            validate(2, 0, 1, 0, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::EmptySize)
        );
        assert_eq!(
            validate(2, 2, 0, 0, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::EmptySize)
        );
    }

    #[test]
    fn short_and_long_buffers_are_rejected() {
        let expected = 2 * 2 * 4 * 3;
        assert_eq!(
            validate(2, 2, 3, expected - 1, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::PixelCount {
                width: 2,
                height: 2,
                layers: 3,
                expected,
                actual: expected - 1,
            })
        );
        assert_eq!(
            validate(2, 2, 3, expected + 1, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::PixelCount {
                width: 2,
                height: 2,
                layers: 3,
                expected,
                actual: expected + 1,
            })
        );
    }

    #[test]
    fn oversized_dimensions_are_rejected() {
        let expected = 9000 * 8 * 4;
        assert_eq!(
            validate(9000, 8, 1, expected, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::TooLarge {
                width: 9000,
                height: 8,
                max: MAX_DIM,
            })
        );
    }

    #[test]
    fn too_many_layers_are_rejected() {
        let layers = MAX_LAYERS + 1;
        let expected = 4 * layers as usize;
        assert_eq!(
            validate(1, 1, layers, expected, MAX_DIM, MAX_LAYERS),
            Err(BlockAtlasError::TooManyLayers {
                layers,
                max: MAX_LAYERS,
            })
        );
    }

    #[test]
    fn overflowing_sizes_are_rejected_without_panicking() {
        assert_eq!(
            validate(u32::MAX, u32::MAX, u32::MAX, 0, u32::MAX, u32::MAX),
            Err(BlockAtlasError::SizeOverflow {
                width: u32::MAX,
                height: u32::MAX,
                layers: u32::MAX,
            })
        );
    }
}
