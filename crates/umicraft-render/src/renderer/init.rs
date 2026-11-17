// SPDX-License-Identifier: MIT OR Apache-2.0

//! Building the renderer: the device, the surface and every pass.

use std::sync::Arc;

use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::TEXTURE_LAYERS;
use umicraft_world::ChunkMap;
use winit::window::Window;

use super::pipelines::{
    create_overlay_pipeline, create_overlay_pipeline_with_entry, create_terrain_pipeline,
    TerrainPipelineState, TERRAIN_WGSL,
};
use super::surface::{
    create_depth, create_offscreen, present_mode, request_adapter, request_device,
    FRAME_LATENCY_UNCONSTRAINED,
};
use super::textures::{create_font_bind_group, create_terrain_bind_group};
use super::Renderer;
use super::{CameraUniform, DEFAULT_MIPMAP_LEVELS, OVERLAY_MIN_BYTES};
use crate::adapter::AdapterSummary;
use crate::batch_buffer::BatchBuffer;
use crate::border::BorderRenderer;
use crate::cave_graph::CaveGraph;
use crate::chunk_pass::{cutout_layers, QuadIndices};
use crate::clouds::CloudRenderer;
use crate::dynamic_blocks::MovingBlocksPass;
use crate::entity_pass::EntityRenderer;
use crate::environment::RenderEnvironment;
use crate::error::RenderError;
use crate::frame_pacing::FramePacingTracker;
use crate::gpu_timer::{DrawCallStats, GpuTimer};
use crate::hires_blocks::HiresBlocks;
use crate::items::{DynamicMesh, GlintPass, IconAtlas, WorldGlint};
use crate::lightmap::TorchFlicker;
use crate::motion_blur::MotionBlur;
use crate::nametag::NametagRenderer;
use crate::outline::OutlineRenderer;
use crate::overlays::{ScreenOverlays, ScreenRenderer};
use crate::panorama::PanoramaRenderer;
use crate::particles::ParticleRenderer;
use crate::scene_blur::SceneBlur;
use crate::sky::SkyRenderer;
use crate::texture::{fallback_texture_data, TEXTURE_SIZE};
use crate::ui_pass::{self, UiPass};
use crate::weather::WeatherRenderer;

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
        umicraft_core::boot_log!("surface_created");
        umicraft_core::boot_log!("adapter_requested");
        let adapter = request_adapter(&instance, Some(&surface))?;
        let info = adapter.get_info();
        umicraft_core::boot_log!(
            "adapter_selected",
            "name='{}', backend={:?}, driver='{}', type={:?}",
            info.name,
            info.backend,
            info.driver,
            info.device_type
        );
        let (device, queue) = request_device(&adapter)?;
        umicraft_core::boot_log!("device_created", "features={:?}", device.features());
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(RenderError::NoSurfaceConfig)?;
        // The 1.8 client does all its colour arithmetic on the stored 8-bit
        // values (texel x biome tint x light, fog mixes, blending) and never
        // asks for an sRGB framebuffer. An sRGB surface would encode those
        // products a second time and wash every colour out, so the surface
        // keeps the plain format and the textures are sampled as stored.
        config.format = config.format.remove_srgb_suffix();
        // The F2 screenshot copies the presented frame, when the surface
        // allows it.
        if surface
            .get_capabilities(&adapter)
            .usages
            .contains(wgpu::TextureUsages::COPY_SRC)
        {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        // The "Max Framerate" option paces the frames; the "Use VSync" option
        // decides whether the display refresh does too (`set_vsync`). Start
        // without, as the first frame has not read the options yet.
        config.present_mode = present_mode(false);
        config.desired_maximum_frame_latency = FRAME_LATENCY_UNCONSTRAINED;
        let format = config.format;
        surface.configure(&device, &config);
        umicraft_core::boot_log!(
            "surface_configured",
            "width={}, height={}, format={:?}, present_mode={:?}, desired_latency={}",
            config.width,
            config.height,
            config.format,
            config.present_mode,
            config.desired_maximum_frame_latency
        );

        let info = adapter.get_info();
        tracing::info!(
            backend = ?info.backend,
            adapter = %info.name,
            vendor = info.vendor,
            device_type = ?info.device_type,
            driver = %info.driver,
            driver_info = %info.driver_info,
            format = ?config.format,
            present_mode = ?config.present_mode,
            desired_maximum_frame_latency = config.desired_maximum_frame_latency,
            width = config.width,
            height = config.height,
            usages = ?config.usage,
            "WGPU presentation surface initialized"
        );

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
            AdapterSummary::of(&adapter),
        )
    }

    /// Creates an offscreen renderer of `width`×`height`.
    ///
    /// The colour target is `Rgba8Unorm`, matching the PNG written by the
    /// client. [`Renderer::read_pixels`] is only available in this mode.
    pub fn new_offscreen(width: u32, height: u32) -> Result<Self, RenderError> {
        let width = width.max(1);
        let height = height.max(1);
        // From the environment, so `WGPU_BACKEND` picks the backend here as it
        // does for the window.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
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
            wgpu::TextureFormat::Rgba8Unorm,
            AdapterSummary::of(&adapter),
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
        adapter: AdapterSummary,
    ) -> Result<Self, RenderError> {
        umicraft_core::boot_log!(
            "renderer_create_started",
            "width={width}, height={height}, format={format:?}"
        );
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
            format: wgpu::TextureFormat::Rgba8Unorm,
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
            // Items, particles and icons never use the mips
            // (`setBlurMipmap(false, false)` in the reference).
            lod_max_clamp: 0.0,
            ..Default::default()
        });
        // The chunks: `GL_NEAREST_MIPMAP_LINEAR` (`setBlurMipmapDirect(false,
        // true)`), nearest texel in each level and a linear blend of the two.
        let mip_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain mip sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_WGSL.into()),
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../overlay.wgsl").into()),
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
                // The large textures: a second array and the map to it.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
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

        let hires = HiresBlocks::none(&device, &queue);
        let terrain_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            &camera_buffer,
            &block_sampler,
            &block_view,
            &hires,
        );
        let local_camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("local camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let local_terrain_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            &local_camera_buffer,
            &block_sampler,
            &block_view,
            &hires,
        );
        let chunk_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            &camera_buffer,
            &mip_sampler,
            &block_view,
            &hires,
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
        let quad_indices = QuadIndices::new(&device);
        let terrain_solid_pipeline = create_terrain_pipeline(
            &device,
            &terrain_shader,
            &terrain_layout,
            format,
            TerrainPipelineState::solid(),
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
        let overlay_pipeline = create_overlay_pipeline(
            &device,
            &overlay_shader,
            &overlay_layout,
            format,
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let nametags = NametagRenderer::new(&device, &overlay_shader, &overlay_layout, format);
        let brand_pipeline = create_overlay_pipeline_with_entry(
            &device,
            &overlay_shader,
            &overlay_layout,
            format,
            wgpu::BlendState::ALPHA_BLENDING,
            "fs_smooth",
        );
        let brand_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("brand font sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let gui_overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../gui_overlay.wgsl").into()),
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
        let gui_overlay_pipeline = create_overlay_pipeline(
            &device,
            &gui_overlay_shader,
            &gui_overlay_layout,
            format,
            wgpu::BlendState::ALPHA_BLENDING,
        );
        // MCP `GuiIngame.renderGameOverlay`: `tryBlendFuncSeparate(775, 769,
        // 1, 0)` for the crosshair, i.e. `src * (1 - dst) + dst * (1 - src)`.
        let gui_inverted_pipeline = create_overlay_pipeline_with_entry(
            &device,
            &gui_overlay_shader,
            &gui_overlay_layout,
            format,
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::OneMinusDst,
                    dst_factor: wgpu::BlendFactor::OneMinusSrc,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            "fs_alpha_test",
        );
        let sky = SkyRenderer::new(&device, &queue, format);
        let clouds = CloudRenderer::new(&device, &queue, format);
        let border = BorderRenderer::new(&device, &queue, format);
        let panorama = PanoramaRenderer::new(&device, format);

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
        let local_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("local entity camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: local_camera_buffer.as_entire_binding(),
            }],
        });
        let hand_camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hand camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let hand_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hand camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: hand_camera_buffer.as_entire_binding(),
            }],
        });
        let entity_renderer = EntityRenderer::new(&device, &camera_layout, format);
        let outline = OutlineRenderer::new(&device, &camera_layout, format);
        let weather = WeatherRenderer::new(&device, &queue, &camera_layout, format);
        let particles = ParticleRenderer::new(&device, &queue, &terrain_layout, format);
        let screen = ScreenRenderer::new(&device, &queue, &terrain_layout, format);

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
        let scene_blur = SceneBlur::new(&device, &queue, (width, height), format);

        let icon_atlas = IconAtlas::new(&device, &queue, format);
        let icon_pipeline = create_terrain_pipeline(
            &device,
            &terrain_shader,
            &terrain_layout,
            format,
            TerrainPipelineState::icon(),
        );
        let icon_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            icon_atlas.camera_buffer(),
            &block_sampler,
            &block_view,
            &hires,
        );
        let icon_entity_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("icon entity camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: icon_atlas.camera_buffer().as_entire_binding(),
            }],
        });
        let world_items = DynamicMesh::new(&device, "world items");
        let moving_blocks = MovingBlocksPass::new(&device);
        let hand_items = DynamicMesh::new(&device, "hand items");
        let hand_item_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            &hand_camera_buffer,
            &block_sampler,
            &block_view,
            &hires,
        );
        let icon_gui_bind_group = create_font_bind_group(
            &device,
            &gui_overlay_layout,
            &font_sampler,
            icon_atlas.view(),
        );
        let glint = GlintPass::new(&device, format);
        let world_glint = WorldGlint::new(&device, &terrain_layout, format);

        let gpu_timer = GpuTimer::new(&device, &queue);
        umicraft_core::boot_log!("renderer_create_completed");
        Ok(Self {
            adapter,
            gpu_timer,
            instance,
            window,
            surface,
            surface_config,
            capture_requested: false,
            captured: None,
            device,
            queue,
            width,
            height,
            depth_view,
            camera_buffer,
            local_camera_buffer,
            local_camera_bind_group,
            local_terrain_bind_group,
            terrain_pipeline,
            terrain_solid_pipeline,
            quad_indices,
            procedural_cutout: cutout_layers(
                &block_data,
                (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize,
            ),
            cutout: cutout_layers(&block_data, (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize),
            cutout_epoch: 0,
            cave_graph: CaveGraph::default(),
            cave_culling: true,
            chunks_version: 0,
            visible_key: None,
            chunk_scratch: Vec::new(),
            terrain_transparent_pipeline,
            crack_pipeline,
            crack_mesh: None,
            moving_blocks,
            hand_camera_buffer,
            hand_camera_bind_group,
            hand: BatchBuffer::default(),
            terrain_bind_group,
            chunk_bind_group,
            mip_sampler,
            mipmap_levels: DEFAULT_MIPMAP_LEVELS,
            terrain_layout,
            visible_solid: Vec::with_capacity(256),
            visible_cutout: Vec::with_capacity(128),
            visible_transparent: Vec::with_capacity(64),
            opaque_candidates: Vec::with_capacity(256),
            transparent_candidates: Vec::with_capacity(64),
            sky,
            clouds,
            cloud_ticks: 0.0,
            torch_flicker: TorchFlicker::new(),
            weather,
            particles,
            particle_sprites: Vec::new(),
            screen,
            screen_overlays: ScreenOverlays::default(),
            border,
            panorama,
            environment: RenderEnvironment::default(),
            block_sampler,
            procedural_blocks: block_view,
            hires,
            official_blocks: None,
            overlay_pipeline,
            nametags,
            brand_pipeline,
            gui_overlay_pipeline,
            gui_inverted_pipeline,
            overlay_layout,
            gui_overlay_layout,
            font_sampler,
            brand_sampler,
            brand_bind_group: None,
            overlay_bind_group,
            ui_bind_group,
            gui_bind_group: None,
            ui_atlas,
            ui_pass: UiPass::new(width, height),
            icon_atlas,
            icon_pipeline,
            icon_bind_group,
            icon_entity_bind_group,
            icon_gui_bind_group,
            glint,
            world_glint,
            icon_mesh: ItemMesh::default(),
            world_items,
            hand_items,
            hand_item_bind_group,
            overlay_buffer,
            overlay_capacity: OVERLAY_MIN_BYTES,
            offscreen,
            chunks: ChunkMap::default(),
            mesh_slots: Vec::with_capacity(512),
            free_mesh_slots: Vec::new(),
            camera_bind_group,
            entity_renderer,
            outline,
            scene_blur,
            motion_blur: MotionBlur::new(format),
            background_blur: false,
            fullbright: false,
            entity_batches: BatchBuffer::default(),
            additive_batches: BatchBuffer::default(),
            glint_batches: BatchBuffer::default(),
            ghost_batches: BatchBuffer::default(),
            apparition_batches: BatchBuffer::default(),
            tinted_batches: BatchBuffer::default(),
            shadow_batches: BatchBuffer::default(),
            last_camera_uniform: None,
            last_local_uniform: None,
            last_hand_uniform: None,
            last_draw_calls: std::cell::Cell::new(DrawCallStats::default()),
            last_present_us: std::cell::Cell::new(0),
            frame_pacing: FramePacingTracker::new(),
        })
    }
}
