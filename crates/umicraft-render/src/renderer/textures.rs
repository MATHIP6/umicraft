// SPDX-License-Identifier: MIT OR Apache-2.0

//! The textures: the block array and its mipmaps, the fonts, the GUI atlas
//! and the enchantment glint.

use umicraft_ui::FontAtlas;

use super::Renderer;
use super::MAX_MIPMAP_LEVELS;
use crate::block_atlas::{self, BlockAtlasError};
use crate::chunk_pass::cutout_layers;
use crate::hires_blocks::{self, HiresBlocks};
use crate::mipmap;
use crate::ui_pass::{self, FontAtlasError};

/// Binds the camera uniform, one block texture-array view, the block sampler
/// and the second array of large textures with its layer map.
///
/// Shared by [`Renderer::create`] (procedural fallback array) and
/// [`Renderer::set_block_atlas`] (installed array), so replacing the terrain
/// texture never duplicates the binding layout.
pub(super) fn create_terrain_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    camera: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
    hires: &HiresBlocks,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("terrain bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(hires.view()),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: hires.map().as_entire_binding(),
            },
        ],
    })
}

/// Binds one R8 font-atlas view and the shared font sampler.
pub(super) fn create_font_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("font bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

impl Renderer {
    /// Uploads the immutable UI font atlas and makes it current for
    /// [`Renderer::render_with_ui`].
    ///
    /// The HUD ([`Overlay::text`](crate::overlay::Overlay::text)) deliberately keeps the built-in fallback
    /// atlas so the M3 overlay and screenshots do not change appearance when
    /// the user installs the 1.8 assets. Call this once at startup, after
    /// load; it performs one texture upload and no blocking wait.
    ///
    /// # Errors
    ///
    /// [`FontAtlasError`] when the pixel buffer does not match the declared
    /// size, the atlas exceeds the device texture limit, or its white UV
    /// rectangle is malformed.
    pub fn set_font_atlas(&mut self, atlas: &FontAtlas) -> Result<(), FontAtlasError> {
        let view = self.upload_font_texture("ui font atlas", atlas)?;
        self.ui_bind_group = create_font_bind_group(
            &self.device,
            &self.overlay_layout,
            &self.font_sampler,
            &view,
        );
        self.ui_atlas = atlas.clone();
        Ok(())
    }

    /// Uploads the brand font atlas (see `umicraft_ui::brand_font`) as a
    /// second font texture, sampled with linear filtering, and hands its
    /// metrics to the UI pass.
    ///
    /// Text drawn with [`umicraft_ui::FontFace::Brand`] uses it from the next
    /// frame on; until this is called such text is drawn with the game font.
    /// Call once at startup; it performs one texture upload and no blocking
    /// wait.
    ///
    /// # Errors
    ///
    /// [`FontAtlasError`] under the same conditions as
    /// [`Renderer::set_font_atlas`]; the previous brand font, if any, stays.
    pub fn set_brand_font(&mut self, atlas: &FontAtlas) -> Result<(), FontAtlasError> {
        let view = self.upload_font_texture("brand font atlas", atlas)?;
        self.brand_bind_group = Some(create_font_bind_group(
            &self.device,
            &self.overlay_layout,
            &self.brand_sampler,
            &view,
        ));
        self.ui_pass.set_brand_atlas(Some(atlas.clone()));
        Ok(())
    }

    /// Validates `atlas` and uploads it as an R8 texture.
    fn upload_font_texture(
        &self,
        label: &str,
        atlas: &FontAtlas,
    ) -> Result<wgpu::TextureView, FontAtlasError> {
        ui_pass::validate_font_atlas(atlas, self.device.limits().max_texture_dimension_2d)?;
        let size = wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width),
                rows_per_image: Some(atlas.height),
            },
            size,
        );
        Ok(texture.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    /// Uploads the RGBA GUI texture atlas for inventory/container backgrounds.
    ///
    /// The atlas is built by `umicraft-assets` from the `textures/gui/` PNGs
    /// in the client jar. Call once at startup, after load. The GUI pipeline
    /// samples this texture for every `DrawCommand::Image` in the UI pass.
    ///
    /// # Errors
    /// [`BlockAtlasError`] when the atlas is empty, its pixel count does not
    /// match or it is larger than the device allows (the GUI of a
    /// high-resolution resource pack); the previous atlas stays bound. The
    /// check runs before any GPU resource is created, as an oversized texture
    /// is a validation error that would stop the client.
    pub fn set_gui_atlas(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), BlockAtlasError> {
        let max = self.device.limits().max_texture_dimension_2d;
        block_atlas::validate(width, height, 1, pixels.len(), max, 1)?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.gui_bind_group = Some(create_font_bind_group(
            &self.device,
            &self.gui_overlay_layout,
            &self.font_sampler,
            &view,
        ));
        Ok(())
    }

    /// The largest width or height of a 2D texture on this device, for the
    /// caller that builds an atlas to fit it.
    #[must_use]
    pub fn max_texture_dimension(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Uploads the enchantment glint texture (`enchanted_item_glint.png`).
    ///
    /// `rgba` must be `width * height * 4` bytes; a malformed upload is
    /// ignored. Until a texture is set, glinting icons and items draw without the
    /// glint.
    pub fn set_glint_texture(&mut self, width: u32, height: u32, rgba: &[u8]) {
        let max = self.device.limits().max_texture_dimension_2d;
        if width > max || height > max {
            // Creating it would be a validation error, which stops the client.
            tracing::error!(width, height, max, "glint texture too large for the device");
            return;
        }
        let uploaded = self.glint.set_texture(
            &self.device,
            &self.queue,
            self.icon_atlas.view(),
            &self.font_sampler,
            (width, height),
            rgba,
        );
        if let Some(view) = uploaded {
            self.world_glint.set_texture(
                &self.device,
                &view,
                self.glint.sampler(),
                self.glint.uniform(),
            );
        }
    }

    /// Sets the clock that scrolls the enchantment glint, in seconds from any
    /// fixed origin. Call once per frame.
    pub fn set_glint_time(&self, seconds: f32) {
        self.glint.set_time(&self.queue, seconds);
    }

    /// Uploads the block/item texture array built from the user's installation
    /// and binds it for terrain rendering.
    ///
    /// `pixels` is layer-major tightly packed RGBA8, exactly
    /// `width × height × 4 × layers` bytes long (the payload of
    /// `umicraft_assets::BlockAtlas`). One `Rgba8Unorm` 2D array texture is
    /// created at the declared layer count and written with a single upload;
    /// the shared nearest-neighbour sampler keeps the texels crisp.
    ///
    /// Until this method succeeds the terrain samples the built-in procedural
    /// fallback (see [`fallback_texture_data`](crate::texture::fallback_texture_data)). A rejected upload leaves that
    /// fallback bound and usable, so a missing or malformed installation
    /// degrades to the M2 look instead of failing. Calling this method again
    /// replaces the previous array.
    ///
    /// # Errors
    ///
    /// [`BlockAtlasError`] when a dimension or the layer count is zero, the
    /// buffer length does not equal `width × height × 4 × layers`, the size
    /// arithmetic overflows, or the layer size or count exceeds the device
    /// limits. Every check runs before any GPU resource is created.
    pub fn set_block_atlas(
        &mut self,
        width: u32,
        height: u32,
        layers: u32,
        pixels: &[u8],
    ) -> Result<(), BlockAtlasError> {
        let limits = self.device.limits();
        block_atlas::validate(
            width,
            height,
            layers,
            pixels.len(),
            limits.max_texture_dimension_2d,
            limits.max_texture_array_layers,
        )?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("block texture array"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1 + mipmap::level_count(width, height, self.mipmap_levels),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.upload_block_layers(&texture, (width, height), pixels);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("block texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // The large textures of the previous array belong to it.
        self.hires = HiresBlocks::none(&self.device, &self.queue);
        self.rebind_block_groups(&view);
        self.atlas_cutout = cutout_layers(pixels, width as usize * height as usize * 4);
        self.apply_opaque_layers();
        self.official_blocks = Some((texture, width, height));
        Ok(())
    }

    /// Draws the block-array `layers` as opaque: their quads skip the alpha
    /// test, so transparent texels show their colour. "Trees: Fast" passes the
    /// leaf textures, as vanilla's fast leaves are drawn in the SOLID layer
    /// (MCP `BlockLeaves.getBlockLayer`, OptiFine `Config.isTreesFancy`). An
    /// empty slice restores the textures' own flags. Meshes uploaded before
    /// the change are drawn with the alpha test until they are uploaded again.
    pub fn set_opaque_layers(&mut self, layers: &[u32]) {
        if self.opaque_layers == layers {
            return;
        }
        self.opaque_layers.clear();
        self.opaque_layers.extend_from_slice(layers);
        self.apply_opaque_layers();
    }

    /// Rebuilds `cutout` from the texture flags and the forced opaque layers.
    fn apply_opaque_layers(&mut self) {
        effective_cutout(&self.atlas_cutout, &self.opaque_layers, &mut self.cutout);
        self.cutout_epoch += 1;
    }

    /// Sets how many mip levels below the base the next
    /// [`Renderer::set_block_atlas`] builds (`GameSettings.mipmapLevels`,
    /// `0..=4`). The current array keeps its levels: like the reference, the
    /// caller reloads the atlas to apply a change.
    pub fn set_mipmap_levels(&mut self, levels: u32) {
        self.mipmap_levels = levels.min(MAX_MIPMAP_LEVELS);
    }

    /// The mip levels the next block atlas is built with.
    #[must_use]
    pub fn mipmap_levels(&self) -> u32 {
        self.mipmap_levels
    }

    /// Writes every layer of `pixels` (layer-major RGBA8) and its mip chain.
    fn upload_block_layers(
        &self,
        texture: &wgpu::Texture,
        (width, height): (u32, u32),
        pixels: &[u8],
    ) {
        let layers = texture.depth_or_array_layers();
        let layer_bytes = width as usize * height as usize * 4;
        let levels = texture.mip_level_count() - 1;
        self.write_block_level(texture, 0, (width, height, layers), pixels);
        if levels == 0 {
            return;
        }
        let level_buffers =
            mipmap::generate_level_buffers(pixels, layer_bytes, (width, height), levels);
        for (idx, buffer) in level_buffers.into_iter().enumerate() {
            let level = (idx + 1) as u32;
            let size = ((width >> level).max(1), (height >> level).max(1), layers);
            self.write_block_level(texture, level, size, &buffer);
        }
    }

    /// One `write_texture` of `size.2` layers of mip `level`.
    fn write_block_level(
        &self,
        texture: &wgpu::Texture,
        level: u32,
        (width, height, layers): (u32, u32, u32),
        pixels: &[u8],
    ) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            // `Queue::write_texture` stages the data internally, so rows need
            // no 256-byte alignment here (unlike texture to buffer copies).
            // The device limit check in `set_block_atlas` caps `width`, so
            // `width * 4` cannot overflow.
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
        );
    }

    /// Rewrites one layer of the official block array (an animation frame).
    ///
    /// `pixels` is one tightly packed RGBA8 layer. Returns `false` and writes
    /// nothing when no official array is bound, the layer does not exist or
    /// the buffer has the wrong length, so a stale animation after a resource
    /// reload cannot corrupt the new array.
    pub fn write_block_layer(&self, layer: u32, pixels: &[u8]) -> bool {
        let Some((texture, width, height)) = &self.official_blocks else {
            return false;
        };
        let expected = u64::from(*width) * u64::from(*height) * 4;
        if layer >= texture.depth_or_array_layers() || pixels.len() as u64 != expected {
            return false;
        }
        // An animation frame replaces the whole mip chain of the layer, as
        // `TextureAtlasSprite.updateAnimation` uploads every level.
        let levels = texture.mip_level_count() - 1;
        for (index, mip) in mipmap::generate_chain(pixels, *width, *height, levels)
            .iter()
            .enumerate()
        {
            let level = index as u32 + 1;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: level,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                mip,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((width >> level).max(1) * 4),
                    rows_per_image: Some((height >> level).max(1)),
                },
                wgpu::Extent3d {
                    width: (width >> level).max(1),
                    height: (height >> level).max(1),
                    depth_or_array_layers: 1,
                },
            );
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(*height),
            },
            wgpu::Extent3d {
                width: *width,
                height: *height,
                depth_or_array_layers: 1,
            },
        );
        true
    }

    /// Binds the built-in procedural block array again.
    ///
    /// Meshes built by the procedural mesher index this array's layers; a
    /// caller that switches back from an official atlas calls this before it
    /// uploads such meshes. The renderer starts on this array, so a call on a
    /// fresh renderer changes nothing.
    pub fn use_procedural_blocks(&mut self) {
        self.official_blocks = None;
        self.atlas_cutout.clone_from(&self.procedural_cutout);
        self.apply_opaque_layers();
        self.hires = HiresBlocks::none(&self.device, &self.queue);
        let view = self.procedural_blocks.clone();
        self.rebind_block_groups(&view);
    }

    /// Binds the block array `view` and the current large textures to every
    /// pass that samples them: the chunks, the local entities and items, and
    /// the icon and hand cameras.
    fn rebind_block_groups(&mut self, view: &wgpu::TextureView) {
        self.terrain_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            &self.camera_buffer,
            &self.block_sampler,
            view,
            &self.hires,
        );
        self.local_terrain_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            &self.local_camera_buffer,
            &self.block_sampler,
            view,
            &self.hires,
        );
        self.chunk_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            &self.camera_buffer,
            &self.mip_sampler,
            view,
            &self.hires,
        );
        self.rebind_icons(view);
    }

    /// Keeps the textures of the official array that are larger than its
    /// layers at their own size, in a second array (see `hires_blocks.rs`).
    ///
    /// `layers` are `(layer of the block array, RGBA8 pixels)` pairs, each
    /// `edge` squared; the block array keeps a shrunk copy of each, which the
    /// shader replaces by the large one. Call it after
    /// [`Renderer::set_block_atlas`], which clears the previous large
    /// textures. Nothing happens without an official array, with no layers, or
    /// with more block layers than the shader's map covers.
    ///
    /// # Errors
    /// [`BlockAtlasError`] when the layers do not fit the device or their
    /// pixel count does not match; the block array stays as it was.
    pub fn set_hires_blocks(
        &mut self,
        edge: u32,
        layers: &[(u32, &[u8])],
    ) -> Result<(), BlockAtlasError> {
        let Some((blocks, _, _)) = &self.official_blocks else {
            return Ok(());
        };
        if layers.is_empty() || blocks.depth_or_array_layers() > hires_blocks::MAX_MAPPED_LAYERS {
            return Ok(());
        }
        let view = blocks.create_view(&wgpu::TextureViewDescriptor {
            label: Some("block texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let count = u32::try_from(layers.len()).unwrap_or(u32::MAX);
        let pixels: Vec<u8> = layers
            .iter()
            .flat_map(|(_, one)| one.iter().copied())
            .collect();
        let limits = self.device.limits();
        block_atlas::validate(
            edge,
            edge,
            count,
            pixels.len(),
            limits.max_texture_dimension_2d,
            limits.max_texture_array_layers,
        )?;
        let levels = mipmap::level_count(edge, edge, self.mipmap_levels);
        let texture = hires_blocks::create_array(&self.device, (edge, edge), count, levels);
        self.upload_block_layers(&texture, (edge, edge), &pixels);
        let pairs: Vec<(u32, u32)> = layers
            .iter()
            .zip(0..)
            .map(|((layer, _), here)| (*layer, here))
            .collect();
        self.hires = HiresBlocks::with_map(&self.device, &self.queue, &texture, &pairs);
        self.rebind_block_groups(&view);
        Ok(())
    }

    /// Points the icon camera at a replaced block array and forgets the
    /// icons rendered from the old one.
    fn rebind_icons(&mut self, block_view: &wgpu::TextureView) {
        self.icon_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            self.icon_atlas.camera_buffer(),
            &self.block_sampler,
            block_view,
            &self.hires,
        );
        self.hand_item_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            &self.hand_camera_buffer,
            &self.block_sampler,
            block_view,
            &self.hires,
        );
        self.icon_atlas.invalidate();
    }
}

/// Writes into `out` the alpha-test flag of every layer: the texture's own
/// flag, except for the `opaque` layers, which never take the test. Layers out
/// of range are ignored.
fn effective_cutout(atlas: &[bool], opaque: &[u32], out: &mut Vec<bool>) {
    out.clear();
    out.extend_from_slice(atlas);
    for &layer in opaque {
        if let Some(flag) = out.get_mut(layer as usize) {
            *flag = false;
        }
    }
}

#[cfg(test)]
mod opaque_layer_tests {
    use super::effective_cutout;

    #[test]
    fn opaque_layers_skip_the_alpha_test_and_others_keep_their_flag() {
        let mut out = Vec::new();
        effective_cutout(&[true, true, false], &[1, 9], &mut out);
        assert_eq!(out, [true, false, false]);
        effective_cutout(&[true, true, false], &[], &mut out);
        assert_eq!(out, [true, true, false]);
    }
}
