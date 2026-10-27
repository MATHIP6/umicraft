// SPDX-License-Identifier: MIT OR Apache-2.0

//! wgpu renderer for chunk meshes, a block texture array, the debug overlay,
//! and the immediate-mode UI pass.
//!
//! [`Renderer::new_windowed`] presents to a winit window; [`Renderer::new_offscreen`]
//! renders to a texture and can save its contents with [`Renderer::read_pixels`].
//! Terrain samples a texture array that starts as the procedural fallback and
//! is replaced by [`Renderer::set_block_atlas`] when the user's installation is
//! available. It is drawn in two passes: opaque faces first with a depth write
//! and alpha cutout, then transparent faces (water, glass, leaves) with depth
//! writes disabled, straight-alpha blending and a farthest-first chunk order.
//! Both passes consume the packed per-vertex block/sky light, baked AO and face
//! shade from the M6 mesh payload, and fog toward the current
//! [`SkyState::fog_color`]. Chunk meshes are uploaded whole and replaced
//! copy-on-write when remeshed. The HUD and the UI share the overlay pipeline;
//! [`Renderer::render_with_ui`] appends [`UiPass`] geometry after the HUD.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use umicraft_mesh::{Mesh, Vertex, TEXTURE_LAYERS};
use umicraft_ui::{DrawList, FontAtlas};
use umicraft_world::ChunkPos;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::block_atlas::{self, BlockAtlasError};
use crate::camera::Camera;
use crate::entity_pass::{EntityRenderer, EntityVertex};
use crate::environment::RenderEnvironment;
use crate::error::RenderError;
use crate::outline::{OutlineBox, OutlineRenderer};
use crate::overlay::{Overlay, OverlayVertex};
use crate::sky::{SkyRenderer, SkyState};
use crate::texture::{fallback_texture_data, TEXTURE_SIZE};
use crate::ui_pass::{self, FontAtlasError, UiBatchKind, UiPass};

/// Depth attachment format. The sky pipeline in [`crate::sky`] uses the same
/// format so it can draw in the main pass.
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Initial overlay vertex-buffer capacity (64 KiB, about 4,000 glyphs).
const OVERLAY_MIN_BYTES: u64 = 64 * 1024;
/// WebGPU requires 256-byte row alignment for buffer↔texture copies.
const ROW_ALIGNMENT: u32 = 256;

const TERRAIN_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Uint32,
    4 => Uint32,
    5 => Uint32
];
const OVERLAY_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    /// Sun-light factor from [`SkyState::sun_brightness`].
    sun_brightness: f32,
    /// Linear fog target from [`SkyState::fog_color`].
    fog_color: [f32; 3],
    /// Start of the fog band in blocks.
    fog_start: f32,
    /// End of the fog band in blocks.
    fog_end: f32,
    /// 1.8 brightness gamma in `0..=1` (Moody → Bright).
    gamma: f32,
    /// Explicit tail padding: the WGSL `Camera` struct is 112 bytes.
    _pad: [f32; 2],
}

/// GPU vertex and index buffers for one pass of a chunk mesh.
struct GpuMeshPass {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// GPU buffers for one chunk column mesh.
///
/// The two passes keep separate buffers so the transparent pass can be drawn
/// after every opaque chunk without touching vertex data. A pass with no faces
/// is absent rather than being a zero-sized buffer (which wgpu rejects).
struct GpuMesh {
    opaque: Option<GpuMeshPass>,
    transparent: Option<GpuMeshPass>,
}

/// One transparent chunk queued for the back-to-front pass.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TransparentDraw {
    pos: ChunkPos,
    distance_sq: f32,
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
    /// Terrain pipeline for water, glass and leaves: no depth write, straight
    /// alpha blending. Drawn after every opaque chunk.
    terrain_transparent_pipeline: wgpu::RenderPipeline,
    crack_pipeline: wgpu::RenderPipeline,
    /// The crack quads of every block being dug, rebuilt by the client.
    crack_mesh: Option<GpuMeshPass>,
    terrain_bind_group: wgpu::BindGroup,
    /// Layout of the terrain camera + texture binding, kept so
    /// [`Renderer::set_block_atlas`] can bind a replacement array without
    /// rebuilding the pipeline.
    terrain_layout: wgpu::BindGroupLayout,
    /// Reused per-frame queue of transparent chunks, farthest first. Its
    /// capacity is bounded by the number of uploaded meshes and is retained
    /// across frames, so sorting allocates nothing in steady state.
    transparent_order: Vec<TransparentDraw>,
    /// Sky, sun/moon and procedural clouds.
    sky: SkyRenderer,
    /// World clock and dimension of the next frame.
    environment: RenderEnvironment,
    /// Nearest-neighbour sampler shared by the fallback and installed arrays.
    block_sampler: wgpu::Sampler,
    /// The built-in procedural block array, kept so
    /// [`Renderer::use_procedural_blocks`] can bind it again after an
    /// official atlas replaced it.
    procedural_blocks: wgpu::TextureView,
    overlay_pipeline: wgpu::RenderPipeline,
    /// Pipeline for GUI texture quads (RGBA, no coverage threshold).
    gui_overlay_pipeline: wgpu::RenderPipeline,
    /// Layout and sampler shared by every font texture, so
    /// [`Renderer::set_font_atlas`] can bind a replacement texture without
    /// recreating the overlay pipeline.
    overlay_layout: wgpu::BindGroupLayout,
    /// Layout for the RGBA GUI atlas, same entries as `overlay_layout` but
    /// paired with `gui_overlay_pipeline`.
    gui_overlay_layout: wgpu::BindGroupLayout,
    font_sampler: wgpu::Sampler,
    /// Fallback-atlas binding kept for the M3 HUD (`Overlay::text`).
    overlay_bind_group: wgpu::BindGroup,
    /// Binding for the UI pass; starts on the fallback atlas and is replaced
    /// once by [`Renderer::set_font_atlas`].
    ui_bind_group: wgpu::BindGroup,
    /// Binding for the RGBA GUI texture atlas; `None` until
    /// [`Renderer::set_gui_atlas`] is called.
    gui_bind_group: Option<wgpu::BindGroup>,
    /// Metrics of the atlas bound to `ui_bind_group`; immutable after upload.
    ui_atlas: FontAtlas,
    ui_pass: UiPass,
    overlay_buffer: wgpu::Buffer,
    overlay_capacity: u64,
    offscreen: Option<OffscreenTarget>,
    chunks: HashMap<ChunkPos, GpuMesh>,
    camera_bind_group: wgpu::BindGroup,
    entity_renderer: EntityRenderer,
    outline: OutlineRenderer,
    entity_batches: Vec<(wgpu::Buffer, u32, wgpu::BindGroup)>,
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

        let terrain_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            &camera_buffer,
            &block_sampler,
            &block_view,
        );
        let overlay_bind_group =
            create_font_bind_group(&device, &overlay_layout, &font_sampler, &font_view);
        let ui_bind_group =
            create_font_bind_group(&device, &overlay_layout, &font_sampler, &font_view);

        let terrain_pipeline = create_terrain_pipeline(
            &device,
            &terrain_shader,
            &terrain_layout,
            format,
            TerrainPipelineState::opaque(),
        );
        let terrain_transparent_pipeline = create_terrain_pipeline(
            &device,
            &terrain_shader,
            &terrain_layout,
            format,
            TerrainPipelineState::transparent(),
        );
        let crack_pipeline = create_terrain_pipeline(
            &device,
            &terrain_shader,
            &terrain_layout,
            format,
            TerrainPipelineState::cracks(),
        );
        let overlay_pipeline =
            create_overlay_pipeline(&device, &overlay_shader, &overlay_layout, format);
        let gui_overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gui_overlay.wgsl").into()),
        });
        let gui_overlay_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gui overlay bind group layout"),
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
        let gui_overlay_pipeline =
            create_overlay_pipeline(&device, &gui_overlay_shader, &gui_overlay_layout, format);
        let sky = SkyRenderer::new(&device, format);

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("entity camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let entity_renderer = EntityRenderer::new(&device, &camera_layout, format);
        let outline = OutlineRenderer::new(&device, &camera_layout, format);

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
            terrain_transparent_pipeline,
            crack_pipeline,
            crack_mesh: None,
            terrain_bind_group,
            terrain_layout,
            transparent_order: Vec::with_capacity(64),
            sky,
            environment: RenderEnvironment::default(),
            block_sampler,
            procedural_blocks: block_view,
            overlay_pipeline,
            gui_overlay_pipeline,
            overlay_layout,
            gui_overlay_layout,
            font_sampler,
            overlay_bind_group,
            ui_bind_group,
            gui_bind_group: None,
            ui_atlas,
            ui_pass: UiPass::new(width, height),
            overlay_buffer,
            overlay_capacity: OVERLAY_MIN_BYTES,
            offscreen,
            chunks: HashMap::new(),
            camera_bind_group,
            entity_renderer,
            outline,
            entity_batches: Vec::new(),
        })
    }

    /// Sets the world clock and dimension used by the sky and fog passes.
    ///
    /// The default is midday in the overworld. Call once per frame before
    /// [`Renderer::render`] or [`Renderer::render_with_ui`]; the value is
    /// plain data and does not allocate.
    pub fn set_environment(&mut self, environment: RenderEnvironment) {
        self.environment = environment;
    }

    /// Replaces the break cracks drawn over the terrain.
    ///
    /// `vertices` and `indices` are the faces of the blocks being dug, with
    /// the layer of their destroy-stage texture; empty slices remove the
    /// cracks.
    pub fn set_block_cracks(&mut self, vertices: &[Vertex], indices: &[u32]) {
        self.crack_mesh = create_gpu_mesh_pass(&self.device, "block cracks", vertices, indices);
    }

    /// Sets the block selection outline for the following frames.
    ///
    /// `boxes` are world-space boxes (the targeted block's selection shape);
    /// an empty slice hides the outline. The outline is drawn after the
    /// terrain and before the HUD, a few pixels wide (see the `outline` module).
    pub fn set_selection_outline(&mut self, boxes: &[OutlineBox]) {
        self.outline
            .set_boxes(&self.queue, boxes, (self.width, self.height));
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
        // One SkyState per frame: the clear colour, the fog uniform and the
        // sky pass must agree even if W3's model changes between calls.
        let sky_state = SkyState::at(self.environment);
        self.write_camera(camera, &sky_state);
        self.sky.update(&self.queue, camera, self.environment);
        self.upload_overlay(overlay);
        self.prepare_transparent_order(camera.position);
        if self.surface.is_some() {
            self.render_window(overlay, &sky_state)
        } else {
            self.render_offscreen(overlay, &sky_state)
        }
    }

    /// Rebuilds the reused transparent draw queue, farthest chunk first.
    ///
    /// Only chunks with a transparent pass are queued; opaque chunks are drawn
    /// straight from the map. The queue never grows beyond the number of
    /// resident meshes, and [`Vec::clear`] keeps its capacity across frames.
    fn prepare_transparent_order(&mut self, camera_position: Vec3) {
        self.transparent_order.clear();
        self.transparent_order
            .extend(self.chunks.iter().filter_map(|(pos, mesh)| {
                mesh.transparent.as_ref().map(|_| TransparentDraw {
                    pos: *pos,
                    distance_sq: squared_distance_to_chunk(*pos, camera_position),
                })
            }));
        sort_transparent_farthest_first(&mut self.transparent_order);
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

    /// Uploads the RGBA GUI texture atlas for inventory/container backgrounds.
    ///
    /// The atlas is built by `umicraft-assets` from the `textures/gui/` PNGs
    /// in the client jar. Call once at startup, after load. The GUI pipeline
    /// samples this texture for every `DrawCommand::Image` in the UI pass.
    pub fn set_gui_atlas(&mut self, width: u32, height: u32, pixels: &[u8]) {
        if width == 0 || height == 0 || pixels.len() != (width as usize * height as usize * 4) {
            return;
        }
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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
    }

    /// Uploads the block/item texture array built from the user's installation
    /// and binds it for terrain rendering.
    ///
    /// `pixels` is layer-major tightly packed RGBA8, exactly
    /// `width × height × 4 × layers` bytes long (the payload of
    /// `umicraft_assets::BlockAtlas`). One `Rgba8UnormSrgb` 2D array texture is
    /// created at the declared layer count and written with a single upload;
    /// the shared nearest-neighbour sampler keeps the texels crisp.
    ///
    /// Until this method succeeds the terrain samples the built-in procedural
    /// fallback (see [`fallback_texture_data`]). A rejected upload leaves that
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
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            // `Queue::write_texture` stages the data internally, so rows need
            // no 256-byte alignment here (unlike texture↔buffer copies). The
            // device limit check above caps `width`, so `width * 4` cannot
            // overflow.
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("block texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        self.terrain_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            &self.camera_buffer,
            &self.block_sampler,
            &view,
        );
        Ok(())
    }

    /// Binds the built-in procedural block array again.
    ///
    /// Meshes built by the procedural mesher index this array's layers; a
    /// caller that switches back from an official atlas calls this before it
    /// uploads such meshes. The renderer starts on this array, so a call on a
    /// fresh renderer changes nothing.
    pub fn use_procedural_blocks(&mut self) {
        self.terrain_bind_group = create_terrain_bind_group(
            &self.device,
            &self.terrain_layout,
            &self.camera_buffer,
            &self.block_sampler,
            &self.procedural_blocks,
        );
    }

    /// Replaces the GPU mesh for `pos`, or removes it when both passes are
    /// empty.
    ///
    /// The opaque and transparent halves are uploaded into separate vertex and
    /// index buffers so the renderer can submit every opaque chunk before the
    /// sorted transparent pass. A half with no faces gets no buffer.
    pub fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        if mesh.is_empty() {
            self.chunks.remove(&pos);
            return;
        }
        let opaque =
            create_gpu_mesh_pass(&self.device, "chunk opaque", &mesh.vertices, &mesh.indices);
        let transparent = create_gpu_mesh_pass(
            &self.device,
            "chunk transparent",
            &mesh.transparent_vertices,
            &mesh.transparent_indices,
        );
        // A pass without indices has no drawable faces. If neither half
        // produced a buffer (including a malformed mesh with vertices but no
        // indices), drop any previous mesh instead of keeping an empty entry.
        if opaque.is_none() && transparent.is_none() {
            self.chunks.remove(&pos);
            return;
        }
        let mesh = GpuMesh {
            opaque,
            transparent,
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

    /// Access to the entity renderer pipeline.
    #[must_use]
    pub fn entity_renderer(&self) -> &EntityRenderer {
        &self.entity_renderer
    }

    /// Access to the underlying wgpu device.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Access to the underlying wgpu queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Uploads entity vertex batches to be drawn in the next frame.
    pub fn upload_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.entity_batches.clear();
        for (vertices, bind_group) in batches {
            if vertices.is_empty() {
                continue;
            }
            let buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("entity batch vertices"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            self.entity_batches
                .push((buffer, vertices.len() as u32, (*bind_group).clone()));
        }
    }

    /// Clears any queued entity batches.
    pub fn clear_entities(&mut self) {
        self.entity_batches.clear();
    }

    /// Creates an entity texture (such as a mob skin or player skin) for entity batches.
    #[must_use]
    pub fn create_entity_texture(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> crate::entity_pass::EntityTexture {
        self.entity_renderer
            .create_texture(&self.device, &self.queue, rgba, width, height)
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

    fn write_camera(&self, camera: &Camera, sky: &SkyState) {
        let (fog_start, fog_end) = fog_range(self.environment.view_distance);
        let uniform = CameraUniform {
            view_proj: camera.view_projection().to_cols_array_2d(),
            camera_pos: camera.position.to_array(),
            sun_brightness: sky.sun_brightness,
            fog_color: sky.fog_color,
            fog_start,
            fog_end,
            gamma: self.environment.gamma,
            _pad: [0.0; 2],
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

    fn render_window(
        &mut self,
        overlay: &Overlay,
        sky_state: &SkyState,
    ) -> Result<(), RenderError> {
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
                self.draw(&view, overlay, &mut encoder, sky_state);
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

    fn render_offscreen(
        &mut self,
        overlay: &Overlay,
        sky_state: &SkyState,
    ) -> Result<(), RenderError> {
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("offscreen encoder"),
            });
        self.draw(&target.view, overlay, &mut encoder, sky_state);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn draw(
        &self,
        view: &wgpu::TextureView,
        overlay: &Overlay,
        encoder: &mut wgpu::CommandEncoder,
        sky_state: &SkyState,
    ) {
        let sky_color = sky_state.sky_color;
        let color_attachment = Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(sky_color[0]),
                    g: f64::from(sky_color[1]),
                    b: f64::from(sky_color[2]),
                    a: 1.0,
                }),
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
        self.sky.draw(&mut pass);

        // Opaque terrain: depth write on, alpha cutout, order-independent.
        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.terrain_bind_group, &[]);
        for mesh in self.chunks.values() {
            if let Some(opaque) = mesh.opaque.as_ref() {
                draw_gpu_pass(&mut pass, opaque);
            }
        }

        // Entities: depth write on, cutout discard, rendered between opaque and transparent passes.
        if !self.entity_batches.is_empty() {
            let batches = self
                .entity_batches
                .iter()
                .map(|(buf, count, bg)| (buf, *count, bg));
            self.entity_renderer
                .render(&mut pass, &self.camera_bind_group, batches);
        }

        // Transparent terrain: water/glass/leaves, farthest chunk first, with
        // depth testing but no depth writes so blended faces do not occlude
        // each other by submission order.
        if !self.transparent_order.is_empty() {
            pass.set_pipeline(&self.terrain_transparent_pipeline);
            pass.set_bind_group(0, &self.terrain_bind_group, &[]);
            for draw in &self.transparent_order {
                let Some(mesh) = self.chunks.get(&draw.pos) else {
                    continue;
                };
                if let Some(transparent) = mesh.transparent.as_ref() {
                    draw_gpu_pass(&mut pass, transparent);
                }
            }
        }

        if let Some(cracks) = self.crack_mesh.as_ref() {
            pass.set_pipeline(&self.crack_pipeline);
            pass.set_bind_group(0, &self.terrain_bind_group, &[]);
            draw_gpu_pass(&mut pass, cracks);
        }

        self.outline.draw(&mut pass, &self.camera_bind_group);

        if !overlay.is_empty() || !self.ui_pass.is_empty() {
            pass.set_vertex_buffer(0, self.overlay_buffer.slice(..));
            // The HUD uses the overlay pipeline with its own fallback atlas.
            if !overlay.is_empty() {
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, &self.overlay_bind_group, &[]);
                pass.draw(0..overlay.vertex_count(), 0..1);
            }
            // UI batches alternate between the font and GUI pipelines.
            let hud_offset = overlay.vertex_count();
            for batch in self.ui_pass.batches() {
                let first = hud_offset + batch.first;
                let end = first + batch.count;
                match batch.kind {
                    UiBatchKind::Font => {
                        pass.set_pipeline(&self.overlay_pipeline);
                        pass.set_bind_group(0, &self.ui_bind_group, &[]);
                    }
                    UiBatchKind::Gui => {
                        if let Some(gui_bg) = &self.gui_bind_group {
                            pass.set_pipeline(&self.gui_overlay_pipeline);
                            pass.set_bind_group(0, gui_bg, &[]);
                        } else {
                            // No GUI atlas uploaded — skip this batch.
                            continue;
                        }
                    }
                }
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

/// Pipeline settings that differ between the opaque and transparent terrain
/// passes. Both share the vertex entry point and bind group layout.
struct TerrainPipelineState {
    label: &'static str,
    fragment_entry: &'static str,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
    blend: wgpu::BlendState,
    bias: wgpu::DepthBiasState,
}

impl TerrainPipelineState {
    /// Opaque pass: closer depth wins, depth is written, alpha cutout runs in
    /// the fragment shader.
    fn opaque() -> Self {
        Self {
            label: "terrain pipeline",
            fragment_entry: "fs_main",
            depth_write: true,
            depth_compare: wgpu::CompareFunction::Less,
            blend: wgpu::BlendState::REPLACE,
            bias: wgpu::DepthBiasState::default(),
        }
    }

    /// Transparent pass: `LessEqual` keeps coincident/behind faces from
    /// disappearing, depth write is off so blended faces accumulate, and
    /// straight alpha is blended (`SrcAlpha`/`OneMinusSrcAlpha`).
    fn transparent() -> Self {
        Self {
            label: "terrain transparent pipeline",
            fragment_entry: "fs_transparent",
            depth_write: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            blend: wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            bias: wgpu::DepthBiasState::default(),
        }
    }

    /// Break cracks: the destroy-stage texture multiplies the colour below
    /// it twice (`DstColor`/`SrcColor`) and is pulled towards the camera so
    /// it does not fight the block faces (MCP
    /// `RenderGlobal.drawBlockDamageTexture`: `tryBlendFuncSeparate(774, 768,
    /// 1, 0)`, `doPolygonOffset(-3, -3)`).
    fn cracks() -> Self {
        Self {
            label: "terrain crack pipeline",
            fragment_entry: "fs_crack",
            depth_write: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            blend: wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Src,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            bias: wgpu::DepthBiasState {
                constant: -3,
                slope_scale: -3.0,
                clamp: 0.0,
            },
        }
    }
}

fn create_terrain_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    state: TerrainPipelineState,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(state.label),
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
            depth_write_enabled: Some(state.depth_write),
            depth_compare: Some(state.depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: state.bias,
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(state.fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(state.blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Uploads one mesh pass, returning `None` when the pass has no drawable
/// geometry.
///
/// Both slices are required: wgpu rejects a zero-sized buffer, and a malformed
/// pass with indices but no vertices cannot be drawn. Valid `umicraft-mesh`
/// output always has four vertices per face.
fn create_gpu_mesh_pass(
    device: &wgpu::Device,
    label: &str,
    vertices: &[Vertex],
    indices: &[u32],
) -> Option<GpuMeshPass> {
    if vertices.is_empty() || indices.is_empty() {
        return None;
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    Some(GpuMeshPass {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    })
}

/// Binds and draws one uploaded mesh pass.
fn draw_gpu_pass(pass: &mut wgpu::RenderPass<'_>, mesh: &GpuMeshPass) {
    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
    pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
}

/// The chunk centre used for transparent sorting: the middle of the 16×16
/// column at the cloud/terrain reference height of y = 128.
fn chunk_centre(pos: ChunkPos) -> Vec3 {
    Vec3::new(pos.x as f32 * 16.0 + 8.0, 128.0, pos.z as f32 * 16.0 + 8.0)
}

/// Squared camera-to-chunk-centre distance; avoids a square root per chunk.
fn squared_distance_to_chunk(pos: ChunkPos, camera_position: Vec3) -> f32 {
    chunk_centre(pos).distance_squared(camera_position)
}

/// Orders transparent chunks farthest first, with `ChunkPos` as the
/// deterministic tie-break (ADR-0008 §3).
fn sort_transparent_farthest_first(chunks: &mut [TransparentDraw]) {
    chunks.sort_unstable_by(|a, b| {
        b.distance_sq
            .total_cmp(&a.distance_sq)
            .then_with(|| a.pos.cmp(&b.pos))
    });
}

/// Fog band `(start, end)` in blocks for a view distance in chunks.
///
/// Uses the ADR-0008 range: start at 75 % of the view distance and end at the
/// full distance capped at 500 blocks (inside the 512-block camera far plane).
/// The start is capped below the end so a degenerate `view_distance` cannot
/// invert the smoothstep edges.
fn fog_range(view_distance: u8) -> (f32, f32) {
    let nominal = f32::from(view_distance) * 16.0;
    let end = nominal.clamp(1.0, 500.0);
    let start = (nominal * 0.75).min(end - 0.001);
    (start, end)
}

/// Binds the camera uniform, one block texture-array view and the block
/// sampler.
///
/// Shared by [`Renderer::create`] (procedural fallback array) and
/// [`Renderer::set_block_atlas`] (installed array), so replacing the terrain
/// texture never duplicates the binding layout.
fn create_terrain_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    camera: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
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
        ],
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
            sun_brightness: 0.5,
            fog_color: [0.25, 0.5, 0.75],
            fog_start: 96.0,
            fog_end: 128.0,
            gamma: 0.5,
            _pad: [0.0; 2],
        };
        let bytes = bytemuck::bytes_of(&uniform);
        assert_eq!(bytes.len(), 112);
        assert_eq!(bytes.len() % 16, 0);
        // These offsets must match the WGSL `Camera` struct in terrain.wgsl.
        assert_eq!(std::mem::offset_of!(CameraUniform, camera_pos), 64);
        assert_eq!(std::mem::offset_of!(CameraUniform, sun_brightness), 76);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_color), 80);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_start), 92);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_end), 96);
    }

    #[test]
    fn fog_range_uses_three_quarters_and_caps_at_500() {
        assert_eq!(fog_range(8), (96.0, 128.0));
        assert_eq!(fog_range(32), (384.0, 500.0));
        // Degenerate and oversized view distances stay a valid band.
        let (start, end) = fog_range(0);
        assert!(start < end);
        let (start, end) = fog_range(255);
        assert!(start < end);
        assert!(end <= 500.0);
    }

    #[test]
    fn transparent_sort_is_farthest_first() {
        let camera = Vec3::ZERO;
        let mut chunks = vec![
            TransparentDraw {
                pos: ChunkPos::new(0, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(0, 0), camera),
            },
            TransparentDraw {
                pos: ChunkPos::new(2, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(2, 0), camera),
            },
            TransparentDraw {
                pos: ChunkPos::new(1, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(1, 0), camera),
            },
        ];
        sort_transparent_farthest_first(&mut chunks);
        assert_eq!(
            chunks.iter().map(|chunk| chunk.pos).collect::<Vec<_>>(),
            vec![
                ChunkPos::new(2, 0),
                ChunkPos::new(1, 0),
                ChunkPos::new(0, 0)
            ]
        );
    }

    #[test]
    fn transparent_sort_ties_break_by_chunk_pos() {
        let camera = chunk_centre(ChunkPos::new(0, 0));
        let mut chunks = [
            ChunkPos::new(1, 0),
            ChunkPos::new(0, -1),
            ChunkPos::new(0, 1),
            ChunkPos::new(-1, 0),
        ]
        .map(|pos| TransparentDraw {
            pos,
            distance_sq: squared_distance_to_chunk(pos, camera),
        });
        // Every chunk centre is exactly 16 blocks from the camera centre.
        assert!(chunks
            .iter()
            .all(|chunk| (chunk.distance_sq - 16.0 * 16.0).abs() < 0.001));
        sort_transparent_farthest_first(&mut chunks);
        assert_eq!(
            chunks.iter().map(|chunk| chunk.pos).collect::<Vec<_>>(),
            vec![
                ChunkPos::new(-1, 0),
                ChunkPos::new(0, -1),
                ChunkPos::new(0, 1),
                ChunkPos::new(1, 0),
            ]
        );
    }

    #[test]
    fn transparent_sort_of_an_empty_list_is_a_no_op() {
        let mut chunks: Vec<TransparentDraw> = Vec::new();
        sort_transparent_farthest_first(&mut chunks);
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_centre_is_the_column_middle() {
        assert_eq!(
            chunk_centre(ChunkPos::new(-1, 2)),
            Vec3::new(-8.0, 128.0, 40.0)
        );
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

    /// The terrain shader is only compiled on a GPU device; this parses and
    /// validates it (all three fragment entry points) without one.
    #[test]
    fn the_terrain_shader_is_valid_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("terrain.wgsl")).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in ["fs_main", "fs_transparent", "fs_crack"] {
            assert!(
                module.entry_points.iter().any(|point| point.name == entry),
                "missing {entry}"
            );
        }
    }
}
