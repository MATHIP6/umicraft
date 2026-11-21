// SPDX-License-Identifier: MIT OR Apache-2.0

//! Building the renderer: the device, the surface and every pass.

use std::sync::Arc;

use umicraft_mesh::item::ItemMesh;
use umicraft_world::ChunkMap;
use winit::window::Window;

use super::pipelines::{
    create_overlay_pipeline, create_overlay_pipeline_with_entry, create_terrain_pipeline,
    TerrainPipelineState, TERRAIN_WGSL,
};
use super::resources;
use super::surface::{
    create_depth, create_offscreen, present_mode, request_adapter, request_device,
    FRAME_LATENCY_UNCONSTRAINED,
};
use super::textures::{create_font_bind_group, create_terrain_bind_group};
use super::Renderer;
use super::{DEFAULT_MIPMAP_LEVELS, OVERLAY_MIN_BYTES};
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
use crate::outline::{LineRenderer, MAX_DEBUG_LINES, SELECTION_EDGES};
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
        let adapter = request_adapter(&instance, Some(&surface))?;
        let (device, queue) = request_device(&adapter)?;
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
        let camera_buffer = resources::camera_buffer(&device, "camera uniform");
        let block_data = fallback_texture_data();
        let block_view = resources::block_texture(&device, &queue, &block_data);

        // The HUD and the UI start on the original 3×5 fallback atlas. A
        // user-installed 1.8 atlas replaces the UI binding only, so the M3
        // overlay keeps the metrics it was built with.
        let ui_atlas = ui_pass::fallback_atlas();
        let font_view = resources::font_texture(&device, &queue, &ui_atlas);
        let font_sampler = resources::font_sampler(&device);
        let block_sampler = resources::block_sampler(&device);
        let mip_sampler = resources::mip_sampler(&device);

        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_WGSL.into()),
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../overlay.wgsl").into()),
        });

        let terrain_layout = resources::terrain_layout(&device);
        let overlay_layout = resources::texture_layout(&device, "overlay bind group layout");

        let hires = HiresBlocks::none(&device, &queue);
        let terrain_bind_group = create_terrain_bind_group(
            &device,
            &terrain_layout,
            &camera_buffer,
            &block_sampler,
            &block_view,
            &hires,
        );
        let local_camera_buffer = resources::camera_buffer(&device, "local camera uniform");
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
        let brand_sampler = resources::brand_sampler(&device);
        let gui_overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../gui_overlay.wgsl").into()),
        });
        let gui_overlay_layout =
            resources::texture_layout(&device, "gui overlay bind group layout");
        let gui_overlay_pipeline = create_overlay_pipeline(
            &device,
            &gui_overlay_shader,
            &gui_overlay_layout,
            format,
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let gui_inverted_pipeline = create_overlay_pipeline_with_entry(
            &device,
            &gui_overlay_shader,
            &gui_overlay_layout,
            format,
            resources::inverted_blend(),
            "fs_alpha_test",
        );
        let sky = SkyRenderer::new(&device, &queue, format);
        let clouds = CloudRenderer::new(&device, &queue, format);
        let border = BorderRenderer::new(&device, &queue, format);
        let panorama = PanoramaRenderer::new(&device, format);

        let camera_layout = resources::camera_layout(&device);
        let camera_bind_group = resources::camera_bind_group(
            &device,
            &camera_layout,
            &camera_buffer,
            "entity camera bind group",
        );
        let local_camera_bind_group = resources::camera_bind_group(
            &device,
            &camera_layout,
            &local_camera_buffer,
            "local entity camera bind group",
        );
        let hand_camera_buffer = resources::camera_buffer(&device, "hand camera uniform");
        let hand_camera_bind_group = resources::camera_bind_group(
            &device,
            &camera_layout,
            &hand_camera_buffer,
            "hand camera bind group",
        );
        let entity_renderer = EntityRenderer::new(&device, &camera_layout, format);
        let outline = LineRenderer::new(&device, &camera_layout, format, SELECTION_EDGES);
        let debug_lines = LineRenderer::new(&device, &camera_layout, format, MAX_DEBUG_LINES);
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
        let icon_entity_bind_group = resources::camera_bind_group(
            &device,
            &camera_layout,
            icon_atlas.camera_buffer(),
            "icon entity camera bind group",
        );
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
            atlas_cutout: cutout_layers(&block_data, (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize),
            opaque_layers: Vec::new(),
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
            debug_lines,
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
