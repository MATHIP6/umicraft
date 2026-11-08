// SPDX-License-Identifier: MIT OR Apache-2.0

//! The second texture array: the block and item textures that are larger than
//! the layers of the main array.
//!
//! Every layer of the main array has the same size, so one large texture (a
//! resource pack's 512 pixel ender pearl) would make all 600 layers that large,
//! about 900 MB. Vanilla avoids it with an item atlas whose sprites keep their
//! own size. Here the few large textures sit in a small second array instead,
//! and a map from main layer to second-array layer tells the shader which one
//! to read: the main array always holds a shrunk copy, so everything that is
//! not aware of the second array (particles, overlays) keeps working.

/// How many main layers the map covers; with more, no second array is used.
pub(crate) const MAX_MAPPED_LAYERS: u32 = 4096;
/// Bytes of the map uniform: one `i32` per mapped layer.
const MAP_BYTES: u64 = MAX_MAPPED_LAYERS as u64 * 4;
/// The map entry of a layer that has no large copy.
const NONE: i32 = -1;

/// The second array and its layer map, bound next to the main array.
#[derive(Debug)]
pub(crate) struct HiresBlocks {
    view: wgpu::TextureView,
    map: wgpu::Buffer,
}

impl HiresBlocks {
    /// Nothing large: a 1x1 placeholder array and a map of `-1`, which the
    /// shader reads as "use the main array".
    pub(crate) fn none(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = create_array(device, (1, 1), 1, 0);
        Self::with_map(device, queue, &texture, &[])
    }

    /// Wraps `texture` and the map of `(main layer, layer here)` pairs.
    pub(crate) fn with_map(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        pairs: &[(u32, u32)],
    ) -> Self {
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("large block texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let map = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("large block texture map"),
            size: MAP_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&map, 0, bytemuck::cast_slice(&layer_map(pairs)));
        Self { view, map }
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub(crate) fn map(&self) -> &wgpu::Buffer {
        &self.map
    }
}

/// The map as the shader reads it: for each main layer the layer of the second
/// array, or `-1`. A pair outside the map is dropped.
pub(crate) fn layer_map(pairs: &[(u32, u32)]) -> Vec<i32> {
    let mut map = vec![NONE; MAX_MAPPED_LAYERS as usize];
    for &(main, here) in pairs {
        if let (Some(slot), Ok(here)) = (map.get_mut(main as usize), i32::try_from(here)) {
            *slot = here;
        }
    }
    map
}

/// An RGBA8 array of `layers` layers of `size`, with `levels` mips below the base.
pub(crate) fn create_array(
    device: &wgpu::Device,
    (width, height): (u32, u32),
    layers: u32,
    levels: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("large block texture array"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1 + levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_points_main_layers_at_their_large_copy_and_the_rest_at_nothing() {
        let map = layer_map(&[(5, 0), (9, 1)]);
        assert_eq!(map.len(), MAX_MAPPED_LAYERS as usize);
        assert_eq!((map[5], map[9]), (0, 1));
        assert_eq!(map.iter().filter(|&&slot| slot != NONE).count(), 2);
    }

    #[test]
    fn a_pair_past_the_end_of_the_map_is_ignored() {
        let map = layer_map(&[(MAX_MAPPED_LAYERS, 0), (3, 2)]);
        assert_eq!(map.iter().filter(|&&slot| slot != NONE).count(), 1);
    }
}
