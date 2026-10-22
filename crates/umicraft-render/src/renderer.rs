// SPDX-License-Identifier: MIT OR Apache-2.0

//! wgpu renderer for chunk meshes, a procedural block texture array, the
//! debug overlay, and the immediate-mode UI pass.
//!
//! [`Renderer::new_windowed`] presents to a winit window; [`Renderer::new_offscreen`]
//! renders to a texture and can save its contents with [`Renderer::read_pixels`].
//! Terrain is drawn with a depth buffer, back-face culling, per-face axis
//! shading, and distance fog. Chunk meshes are uploaded whole and replaced
//! copy-on-write when remeshed. The HUD and the UI share the overlay pipeline;
//! [`Renderer::render_with_ui`] appends [`UiPass`] geometry after the HUD.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use umicraft_mesh::{Mesh, Vertex, TEXTURE_LAYERS};
use umicraft_ui::{DrawList, FontAtlas};
use umicraft_world::ChunkPos;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::Camera;
use crate::error::RenderError;
use crate::overlay::{Overlay, OverlayVertex};
use crate::texture::{fallback_texture_data, TEXTURE_SIZE};
use crate::ui_pass::{self, FontAtlasError, UiPass};

/// Depth attachment format.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Clear colour and fog target. Linear values; the sRGB target converts them
/// to displayed values.
const SKY: wgpu::Color = wgpu::Color {
    r: 0.55,
    g: 0.72,
    b: 0.95,
    a: 1.0,
};
/// Initial overlay vertex-buffer capacity (64 KiB, about 4,000 glyphs).
const OVERLAY_MIN_BYTES: u64 = 64 * 1024;
/// WebGPU requires 256-byte row alignment for buffer↔texture copies.
const ROW_ALIGNMENT: u32 = 256;

const TERRAIN_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Uint32];
const OVERLAY_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    _pad: f32,
}

/// RGBA8 pixels returned by [`Renderer::read_pixels`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Tightly packed RGBA8 rows, top to bottom.
    pub rgba: Vec<u8>,
}

/// GPU buffers for one chunk column mesh.
struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// Offscreen colour target and the texture retained for readback.
struct OffscreenTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Windowed or offscreen wgpu renderer.
pub struct Renderer {
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
    depth_view: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_bind_group: wgpu::BindGroup,
    overlay_pipeline: wgpu::RenderPipeline,
    /// Layout and sampler shared by every font texture, so
    /// [`Renderer::set_font_atlas`] can bind a replacement texture without
    /// recreating the overlay pipeline.
    overlay_layout: wgpu::BindGroupLayout,
    font_sampler: wgpu::Sampler,
    /// Fallback-atlas binding kept for the M3 HUD (`Overlay::text`).
    overlay_bind_group: wgpu::BindGroup,
    /// Binding for the UI pass; starts on the fallback atlas and is replaced
    /// once by [`Renderer::set_font_atlas`].
    ui_bind_group: wgpu::BindGroup,
    /// Metrics of the atlas bound to `ui_bind_group`; immutable after upload.
    ui_atlas: FontAtlas,
    ui_pass: UiPass,
    overlay_buffer: wgpu::Buffer,
    overlay_capacity: u64,
    offscreen: Option<OffscreenTarget>,
    chunks: HashMap<ChunkPos, GpuMesh>,
}

impl Renderer {
    /// Creates a renderer that presents to `window`.
    ///
    /// `instance` must be created with the window's display handle, e.g.
    /// `InstanceDescriptor::new_with_display_handle_from_env`. The surface is
    /// configured with the adapter's preferred format and vsync behaviour.
    pub fn new_windowed(
        instance: wgpu::Instance,
        window: Arc<Window>,
    ) -> Result<Self, RenderError> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| RenderError::Surface(format!("{error:?}")))?;
        let adapter = request_adapter(&instance, Some(&surface))?;
        let (device, queue) = request_device(&adapter)?;
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(RenderError::NoSurfaceConfig)?;
        let format = config.format;
        surface.configure(&device, &config);
        Self::create(
            instance,
            Some(window),
            Some(surface),
            Some(config),
            device,
            queue,
            width,
            height,
            format,
        )
    }

    /// Creates an offscreen renderer of `width`×`height`.
    ///
    /// The colour target is `Rgba8UnormSrgb`, matching the PNG written by the
    /// client. [`Renderer::read_pixels`] is only available in this mode.
    pub fn new_offscreen(width: u32, height: u32) -> Result<Self, RenderError> {
        let width = width.max(1);
        let height = height.max(1);
        let instance = wgpu::Instance::default();
        let adapter = request_adapter(&instance, None)?;
        let (device, queue) = request_device(&adapter)?;
        Self::create(
            instance,
            None,
            None,
            None,
            device,
            queue,
            width,
            height,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create(
        instance: wgpu::Instance,
        window: Option<Arc<Window>>,
        surface: Option<wgpu::Surface<'static>>,
        surface_config: Option<wgpu::SurfaceConfiguration>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RenderError> {
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let block_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("block texture array"),
            size: wgpu::Extent3d {
                width: TEXTURE_SIZE,
                height: TEXTURE_SIZE,
                depth_or_array_layers: TEXTURE_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let block_data = fallback_texture_data();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &block_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &block_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TEXTURE_SIZE * 4),
                rows_per_image: Some(TEXTURE_SIZE),
            },
            wgpu::Extent3d {
                width: TEXTURE_SIZE,
                height: TEXTURE_SIZE,
                depth_or_array_layers: TEXTURE_LAYERS,
            },
        );
        let block_view = block_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("block texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        // The HUD and the UI start on the original 3×5 fallback atlas. A
        // user-installed 1.8 atlas replaces the UI binding only, so the M3
        // overlay keeps the metrics it was built with.
        let ui_atlas = ui_pass::fallback_atlas();
        let font_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overlay font atlas"),
            size: wgpu::Extent3d {
                width: ui_atlas.width,
                height: ui_atlas.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &font_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &ui_atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ui_atlas.width),
                rows_per_image: Some(ui_atlas.height),
            },
            wgpu::Extent3d {
                width: ui_atlas.width,
                height: ui_atlas.height,
                depth_or_array_layers: 1,
            },
        );
        let font_view = font_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let font_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("font sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let block_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("block sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("overlay.wgsl").into()),
        });

        let terrain_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let overlay_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let terrain_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain bind group"),
            layout: &terrain_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&block_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&block_sampler),
                },
            ],
        });
        let overlay_bind_group =
            create_font_bind_group(&device, &overlay_layout, &font_sampler, &font_view);
        let ui_bind_group =
            create_font_bind_group(&device, &overlay_layout, &font_sampler, &font_view);

        let terrain_pipeline =
            create_terrain_pipeline(&device, &terrain_shader, &terrain_layout, format);
        let overlay_pipeline =
            create_overlay_pipeline(&device, &overlay_shader, &overlay_layout, format);

        let overlay_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay vertices"),
            size: OVERLAY_MIN_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let offscreen = if surface.is_none() {
            Some(create_offscreen(&device, width, height))
        } else {
            None
        };
        let depth_view = create_depth(&device, width, height);

        Ok(Self {
            instance,
            window,
            surface,
            surface_config,
            device,
            queue,
            width,
            height,
            depth_view,
            camera_buffer,
            terrain_pipeline,
            terrain_bind_group,
            overlay_pipeline,
            overlay_layout,
            font_sampler,
            overlay_bind_group,
            ui_bind_group,
            ui_atlas,
            ui_pass: UiPass::new(width, height),
            overlay_buffer,
            overlay_capacity: OVERLAY_MIN_BYTES,
            offscreen,
            chunks: HashMap::new(),
        })
    }

    /// Draws one frame with the M3 HUD overlay only.
    ///
    /// Any UI geometry from a previous [`Renderer::render_with_ui`] call is
    /// discarded, so the two entry points can be interleaved safely. Skips
    /// presentation on timeout or occlusion and reconfigures the surface when
    /// the backend requests it.
    pub fn render(&mut self, camera: &Camera, overlay: &Overlay) -> Result<(), RenderError> {
        self.ui_pass.clear();
        self.render_frame(camera, overlay)
    }

    /// Draws one frame: terrain, then the `overlay` HUD, then `draw_list`.
    ///
    /// The draw list is built by the caller (on this same render thread)
    /// before the call; the renderer only converts it to vertices and submits
    /// them. It uses the atlas most recently passed to
    /// [`Renderer::set_font_atlas`], or the built-in fallback atlas when none
    /// was set. The UI is drawn above the HUD and shares the overlay pipeline,
    /// so no extra pass or thread is introduced.
    pub fn render_with_ui(
        &mut self,
        camera: &Camera,
        overlay: &Overlay,
        draw_list: &DrawList,
    ) -> Result<(), RenderError> {
        self.ui_pass.rebuild(draw_list, &self.ui_atlas);
        self.render_frame(camera, overlay)
    }

    fn render_frame(&mut self, camera: &Camera, overlay: &Overlay) -> Result<(), RenderError> {
        self.write_camera(camera);
        self.upload_overlay(overlay);
        if self.surface.is_some() {
            self.render_window(overlay)
        } else {
            self.render_offscreen(overlay)
        }
    }

    /// Uploads the immutable UI font atlas and makes it current for
    /// [`Renderer::render_with_ui`].
    ///
    /// The HUD ([`Overlay::text`]) deliberately keeps the built-in fallback
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
        ui_pass::validate_font_atlas(atlas, self.device.limits().max_texture_dimension_2d)?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui font atlas"),
            size: wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
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
            wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.ui_bind_group = create_font_bind_group(
            &self.device,
            &self.overlay_layout,
            &self.font_sampler,
            &view,
        );
        self.ui_atlas = atlas.clone();
        Ok(())
    }

    /// Replaces the GPU mesh for `pos`, or removes it when `mesh` is empty.
    pub fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        if mesh.is_empty() {
            self.chunks.remove(&pos);
            return;
        }
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunk vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunk indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let index_count = mesh.indices.len() as u32;
        let mesh = GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        if let Entry::Occupied(mut entry) = self.chunks.entry(pos) {
            *entry.get_mut() = mesh;
        } else {
            self.chunks.insert(pos, mesh);
        }
    }

    /// Removes the GPU mesh for one chunk column.
    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    /// Removes every uploaded chunk mesh (e.g. after a dimension change).
    pub fn clear_meshes(&mut self) {
        self.chunks.clear();
    }

    /// Number of chunk meshes currently resident on the GPU.
    #[must_use]
    pub fn mesh_count(&self) -> usize {
        self.chunks.len()
    }

    /// Resizes the swap chain (windowed) or offscreen colour target.
    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        let device = &self.device;
        if let Some(config) = self.surface_config.as_mut() {
            config.width = width;
            config.height = height;
            if let Some(surface) = self.surface.as_ref() {
                surface.configure(device, config);
            }
        }
        self.depth_view = create_depth(&self.device, width, height);
        self.ui_pass.resize(width, height);
        if self.offscreen.is_some() {
            self.offscreen = Some(create_offscreen(&self.device, width, height));
        }
    }

    /// Current target size in physical pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Reads the offscreen colour target as tightly packed RGBA8.
    ///
    /// Call [`Renderer::render`] or [`Renderer::render_with_ui`] first. The
    /// row padding required by WebGPU is removed before returning.
    pub fn read_pixels(&self) -> Result<Image, RenderError> {
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        let bytes_per_row = self.width * 4;
        let padded_row = bytes_per_row.div_ceil(ROW_ALIGNMENT) * ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let mapping = Arc::new(Mutex::new(None));
        let signal = Arc::clone(&mapping);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if let Ok(mut slot) = signal.lock() {
                    *slot = Some(result);
                }
            });
        self.device
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
        let mut rgba = Vec::with_capacity((bytes_per_row * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded_row) as usize;
            let end = start + bytes_per_row as usize;
            rgba.extend_from_slice(&mapped[start..end]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(Image {
            width: self.width,
            height: self.height,
            rgba,
        })
    }

    fn write_camera(&self, camera: &Camera) {
        let uniform = CameraUniform {
            view_proj: camera.view_projection().to_cols_array_2d(),
            camera_pos: camera.position.to_array(),
            _pad: 0.0,
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn upload_overlay(&mut self, overlay: &Overlay) {
        let hud_bytes: &[u8] = bytemuck::cast_slice(overlay.vertices());
        let hud_len = hud_bytes.len();
        let ui_len = std::mem::size_of_val(self.ui_pass.vertices());
        let total = hud_len + ui_len;
        if total as u64 > self.overlay_capacity {
            let capacity = (total as u64).next_power_of_two().max(OVERLAY_MIN_BYTES);
            self.overlay_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("overlay vertices"),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.overlay_capacity = capacity;
        }
        if !hud_bytes.is_empty() {
            self.queue.write_buffer(&self.overlay_buffer, 0, hud_bytes);
        }
        if ui_len != 0 {
            let ui_bytes: &[u8] = bytemuck::cast_slice(self.ui_pass.vertices());
            self.queue
                .write_buffer(&self.overlay_buffer, hud_len as u64, ui_bytes);
        }
    }

    fn render_window(&mut self, overlay: &Overlay) -> Result<(), RenderError> {
        let frame = {
            let surface = self
                .surface
                .as_ref()
                .ok_or_else(|| RenderError::Surface("renderer has no surface".into()))?;
            surface.get_current_texture()
        };
        match frame {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("frame encoder"),
                        });
                self.draw(&view, overlay, &mut encoder);
                self.queue.submit(Some(encoder.finish()));
                if let Some(window) = &self.window {
                    window.pre_present_notify();
                }
                self.queue.present(frame);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                self.reconfigure_surface()?;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => Ok(()),
            wgpu::CurrentSurfaceTexture::Lost => {
                self.recreate_surface()?;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Validation => Err(RenderError::SurfaceLost),
        }
    }

    fn render_offscreen(&mut self, overlay: &Overlay) -> Result<(), RenderError> {
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("offscreen encoder"),
            });
        self.draw(&target.view, overlay, &mut encoder);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn draw(
        &self,
        view: &wgpu::TextureView,
        overlay: &Overlay,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let color_attachment = Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(SKY),
                store: wgpu::StoreOp::Store,
            },
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main pass"),
            color_attachments: &[color_attachment],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.terrain_bind_group, &[]);
        for mesh in self.chunks.values() {
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
        if !overlay.is_empty() || !self.ui_pass.is_empty() {
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_vertex_buffer(0, self.overlay_buffer.slice(..));
            // Both vertex ranges live in the same buffer, in submission
            // order. The HUD and the UI may use different atlases, so they
            // are two draw calls with different bind groups.
            if !overlay.is_empty() {
                pass.set_bind_group(0, &self.overlay_bind_group, &[]);
                pass.draw(0..overlay.vertex_count(), 0..1);
            }
            if !self.ui_pass.is_empty() {
                pass.set_bind_group(0, &self.ui_bind_group, &[]);
                let first = overlay.vertex_count();
                let end = first + self.ui_pass.vertex_count();
                pass.draw(first..end, 0..1);
            }
        }
    }

    fn reconfigure_surface(&mut self) -> Result<(), RenderError> {
        let config = self
            .surface_config
            .as_ref()
            .ok_or_else(|| RenderError::Surface("renderer has no surface config".into()))?;
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| RenderError::Surface("renderer has no surface".into()))?;
        surface.configure(&self.device, config);
        Ok(())
    }

    fn recreate_surface(&mut self) -> Result<(), RenderError> {
        let window = self
            .window
            .clone()
            .ok_or_else(|| RenderError::Surface("renderer has no window".into()))?;
        let config = self
            .surface_config
            .as_ref()
            .ok_or_else(|| RenderError::Surface("renderer has no surface config".into()))?;
        let surface = self
            .instance
            .create_surface(window)
            .map_err(|error| RenderError::Surface(format!("{error:?}")))?;
        surface.configure(&self.device, config);
        self.surface = Some(surface);
        Ok(())
    }
}

fn request_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<wgpu::Adapter, RenderError> {
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        apply_limit_buckets: false,
    }))
    .map_err(|error| RenderError::NoAdapter(format!("{error:?}")))
}

fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), RenderError> {
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("umicraft device"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| RenderError::NoDevice(format!("{error:?}")))
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_offscreen(device: &wgpu::Device, width: u32, height: u32) -> OffscreenTarget {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen colour"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    OffscreenTarget { texture, view }
}

fn create_terrain_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("terrain pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &TERRAIN_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Binds one R8 font-atlas view and the shared font sampler.
fn create_font_bind_group(
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

fn create_overlay_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("overlay pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("overlay pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<OverlayVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &OVERLAY_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_uniform_is_pod_and_aligned() {
        let uniform = CameraUniform {
            view_proj: [[0.0; 4]; 4],
            camera_pos: [1.0, 2.0, 3.0],
            _pad: 0.0,
        };
        let bytes = bytemuck::bytes_of(&uniform);
        assert_eq!(bytes.len(), 80);
        assert_eq!(bytes.len() % 16, 0);
    }

    #[test]
    fn image_is_tightly_packed() {
        let image = Image {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn offscreen_smoke_renders_sky_and_overlay() {
        // 70 px rows are 280 bytes, so the readback must strip WebGPU's
        // 256-byte row padding (512 bytes per row).
        let mut renderer = Renderer::new_offscreen(70, 48).expect("offscreen renderer");
        let camera = Camera::default();
        let mut overlay = Overlay::new(70, 48);
        overlay.text(2.0, 2.0, 1.0, [1.0; 4], "OK");
        renderer.render(&camera, &overlay).expect("render");
        let image = renderer.read_pixels().expect("readback");
        assert_eq!(image.rgba.len(), 70 * 48 * 4);
        let first = &image.rgba[..3];
        assert!(image.rgba.chunks_exact(4).any(|pixel| &pixel[..3] != first));
    }
}
