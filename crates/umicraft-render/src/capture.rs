// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reads a GPU texture back to the CPU as tightly packed RGBA8: the offscreen
//! screenshot path and the F2 screenshot of the window share it.

use std::sync::{Arc, Mutex};

use crate::error::RenderError;
use crate::renderer::Image;

/// WebGPU requires 256-byte row alignment for buffer to texture copies.
const ROW_ALIGNMENT: u32 = 256;

/// Copies `texture` (4 bytes per pixel, `size` pixels) into a buffer, maps
/// it and removes the row padding. `blue_first` swaps the red and blue
/// channels of a `Bgra8` surface so the result is always RGBA.
///
/// # Errors
/// [`RenderError::Readback`] when the device cannot map the buffer.
pub(crate) fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    (width, height): (u32, u32),
    blue_first: bool,
) -> Result<Image, RenderError> {
    let bytes_per_row = width * 4;
    let padded_row = bytes_per_row.div_ceil(ROW_ALIGNMENT) * ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let mapping = Arc::new(Mutex::new(None));
    let signal = Arc::clone(&mapping);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            if let Ok(mut slot) = signal.lock() {
                *slot = Some(result);
            }
        });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| RenderError::Readback(format!("{error:?}")))?;
    {
        let slot = mapping
            .lock()
            .map_err(|_| RenderError::Readback("mapping state was poisoned".into()))?;
        let result = slot
            .as_ref()
            .ok_or_else(|| RenderError::Readback("mapping callback did not run".into()))?;
        result
            .as_ref()
            .map_err(|error| RenderError::Readback(format!("{error:?}")))?;
    }

    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|error| RenderError::Readback(format!("{error:?}")))?;
    let mut rgba = Vec::with_capacity((bytes_per_row * height) as usize);
    for row in 0..height {
        let start = (row * padded_row) as usize;
        let end = start + bytes_per_row as usize;
        rgba.extend_from_slice(&mapped[start..end]);
    }
    drop(mapped);
    buffer.unmap();
    if blue_first {
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    Ok(Image {
        width,
        height,
        rgba,
    })
}
