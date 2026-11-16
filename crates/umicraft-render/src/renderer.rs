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
//! [`SkyState::fog`]. Chunk meshes are uploaded whole and replaced
//! copy-on-write when remeshed. The HUD and the UI share the overlay pipeline;
//! [`Renderer::render_with_ui`] appends [`UiPass`] geometry after the HUD.

use crate::lightmap::{self, LightmapParams, TorchFlicker};
use crate::nametag::{Nametag, NametagRenderer};
use std::collections::hash_map::Entry;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec3};
use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::visibility::Connectivity;
use umicraft_mesh::{Mesh, Vertex, VertexSpan, TEXTURE_LAYERS};
use umicraft_ui::menus::{PanoramaTextures, PanoramaView};
use umicraft_ui::{DrawCommand, DrawList, FontAtlas};
use umicraft_world::{ChunkMap, ChunkPos};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::adapter::AdapterSummary;
use crate::batch_buffer::BatchBuffer;
use crate::block_atlas::{self, BlockAtlasError};
use crate::border::{BorderRenderer, BorderView};
use crate::camera::{Camera, Frustum};
use crate::capture;
use crate::cave_graph::CaveGraph;
use crate::chunk_pass::{
    create_chunk_pass, cutout_layers, ChunkPass, Group, QuadIndices, SECTIONS,
};
use crate::clouds::{CloudMode, CloudRenderer};
use crate::dynamic_blocks::MovingBlocksPass;
use crate::entity_pass::{EntityRenderer, EntityVertex};
use crate::environment::RenderEnvironment;
use crate::error::RenderError;
use crate::frame_pacing::{FramePacingStats, FramePacingTracker, SurfaceInfo};
use crate::gpu_timer::{self, DrawCallStats, GpuTimer, GpuTimings, VramStats};
use crate::hires_blocks::{self, HiresBlocks};
use crate::items::{DynamicMesh, GlintPass, IconAtlas, IconProvider, WorldGlint};
use crate::mipmap;
use crate::motion_blur::{MotionBlur, MotionBlurSettings};
use crate::outline::{OutlineBox, OutlineRenderer};
use crate::overlay::{Overlay, OverlayVertex};
use crate::overlays::{Pictures, ScreenOverlays, ScreenRenderer};
use crate::panorama::PanoramaRenderer;
use crate::particles::{ParticleRenderer, ParticleSprite};
use crate::scene_blur::SceneBlur;
use crate::sky::{SkyImage, SkyRenderer, SkyState, SkyTextures};
use crate::texture::{fallback_texture_data, TEXTURE_SIZE};
use crate::ui_pass::{self, FontAtlasError, UiBatchKind, UiPass};
use crate::weather::{PrecipitationField, WeatherRenderer};

/// Depth attachment format. The sky pipeline in [`crate::sky`] uses the same
/// format so it can draw in the main pass.
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Initial overlay vertex-buffer capacity (64 KiB, about 4,000 glyphs).
const OVERLAY_MIN_BYTES: u64 = 64 * 1024;
/// Initial capacity of the per-frame item buffers.
pub(crate) const MIN_DYNAMIC_BYTES: u64 = 16 * 1024;

/// The highest `Mipmap Levels` option (`GameSettings`, slider 0 to 4).
pub(crate) const MAX_MIPMAP_LEVELS: u32 = 4;
/// The reference default (`GameSettings.mipmapLevels = 4`).
const DEFAULT_MIPMAP_LEVELS: u32 = 4;

pub(crate) const TERRAIN_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Uint32,
    4 => Uint32,
    5 => Uint32
];
pub(crate) const OVERLAY_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// Half the depth range of the icon camera, in GUI pixels; a lit block's
/// rotated corners stay within about 17.
const ICON_DEPTH: f32 = 32.0;
/// The terrain shader with the shared fog falloff in front.
const TERRAIN_WGSL: &str = concat!(
    include_str!("fog.wgsl"),
    include_str!("lightmap.wgsl"),
    include_str!("terrain.wgsl")
);

/// Frames the presentation queue may hold (F2-5e). When vsync is active, 1 keeps
/// input latency minimal; when unconstrained without vsync, 2 lets the CPU pipeline
/// ahead of the presentation engine to maximise extreme framerates.
const FRAME_LATENCY_VSYNC: u32 = 1;
const FRAME_LATENCY_UNCONSTRAINED: u32 = 2;

/// A fog distance the hand never reaches.
const HAND_FOG_FREE: f32 = 1.0e6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// The eye relative to `origin` (small), so the fog distance is computed
    /// from two small numbers (F2-1c).
    camera_pos: [f32; 3],
    /// Sun-light factor from [`SkyState::sun_brightness`].
    sun_brightness: f32,
    /// Linear fog target from [`SkyState::fog`].
    fog_color: [f32; 3],
    /// Start of the linear fog band in blocks.
    fog_start: f32,
    /// End of the linear fog band in blocks.
    fog_end: f32,
    /// 1.8 brightness gamma in `0..=1` (Moody → Bright).
    gamma: f32,
    /// Explicit tail padding: the WGSL `Camera` struct is 112 bytes.
    /// Exponential fog density (water, lava); above zero it replaces the
    /// linear band (`fog_amount` in `fog.wgsl`).
    fog_density: f32,
    _pad: f32,
    /// Integer world offset the matrix is relative to: shaders subtract it
    /// from every position, which keeps f32 exact far from the origin.
    origin: [f32; 3],
    _pad_origin: f32,
    /// The vanilla lightmap, `[sky * 16 + block]` (see [`crate::lightmap`]).
    lightmap: [[f32; 4]; lightmap::ENTRIES],
}

impl CameraUniform {
    /// The icon camera: an orthographic view of the 16x16 GUI box (y down,
    /// larger z nearer), no fog, full daylight and no gamma lift.
    pub(crate) fn icon() -> Self {
        Self {
            // x: 0..16 to -1..1; y flips (down to up); z: -D..D to 1..0, so a
            // larger z is nearer, as in the reference GUI projection
            // (glOrtho 1000..3000 after translate(0, 0, -2000)).
            view_proj: (glam::Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.5))
                * glam::Mat4::from_scale(Vec3::new(1.0 / 8.0, -1.0 / 8.0, -0.5 / ICON_DEPTH)))
            .to_cols_array_2d(),
            camera_pos: [0.0; 3],
            sun_brightness: 1.0,
            fog_color: [0.0; 3],
            fog_start: HAND_FOG_FREE,
            fog_end: HAND_FOG_FREE * 2.0,
            gamma: 0.0,
            fog_density: 0.0,
            _pad: 0.0,
            origin: [0.0; 3],
            _pad_origin: 0.0,
            // GUI items are drawn without the lightmap.
            lightmap: lightmap::unlit(),
        }
    }
}

/// GPU vertex and index buffers for one pass of a chunk mesh.
pub(crate) struct GpuMeshPass {
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
    opaque: Option<ChunkPass>,
    transparent: Option<ChunkPass>,
    /// Which faces of each section see each other, for cave culling; all open
    /// for a mesh that came without sections.
    connectivity: [Connectivity; SECTIONS],
}

/// One chunk queued for a sorted pass: opaque ones go near to far (less
/// overdraw), transparent ones far to near.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct ChunkDraw {
    pos: ChunkPos,
    distance_sq: f32,
    /// The sections to draw (bit `s` = section `s`): in the frustum and not
    /// hidden by the cave graph.
    sections: u16,
    has_solid: bool,
    has_cutout: bool,
}

/// Up to 4 contiguous index ranges per chunk draw, stack-inline without heap allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChunkRuns {
    pub ranges: [std::ops::Range<u32>; 4],
    pub count: u8,
}

impl ChunkRuns {
    #[inline(always)]
    pub fn empty() -> Self {
        Self {
            ranges: [0..0, 0..0, 0..0, 0..0],
            count: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        let c = self.count as usize;
        if c > 0 && self.ranges[c - 1].end == range.start {
            self.ranges[c - 1].end = range.end;
            return;
        }
        if c < self.ranges.len() {
            self.ranges[c] = range;
            self.count += 1;
        } else if c > 0 {
            self.ranges[c - 1].end = range.end;
        }
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[std::ops::Range<u32>] {
        &self.ranges[..self.count as usize]
    }
}

/// A pre-resolved draw command for a chunk pass: direct mesh slot and pre-collected index ranges.
#[derive(Debug, Clone, PartialEq)]
struct PreparedChunkDraw {
    slot: u32,
    runs: ChunkRuns,
}

struct OpaqueCandidate {
    slot: u32,
    pos: ChunkPos,
    distance_sq: f32,
    sections: u16,
    has_cutout: bool,
}

struct TransparentCandidate {
    slot: u32,
    pos: ChunkPos,
    distance_sq: f32,
    sections: u16,
}

struct MeshSlot {
    pos: ChunkPos,
    mesh: GpuMesh,
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
    adapter: AdapterSummary,
    gpu_timer: Option<GpuTimer>,
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    /// A screenshot of the next presented frame was requested.
    capture_requested: bool,
    /// The last captured frame, waiting for [`Renderer::take_capture`].
    captured: Option<Result<Image, RenderError>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
    depth_view: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    /// The camera for passes whose vertices are already relative to the
    /// render origin (entities, particles, dropped items): the same matrix,
    /// with `origin` zero so the shader subtracts nothing (F2-1b).
    local_camera_buffer: wgpu::Buffer,
    local_camera_bind_group: wgpu::BindGroup,
    /// The block array with [`Renderer::local_camera_buffer`].
    local_terrain_bind_group: wgpu::BindGroup,
    terrain_pipeline: wgpu::RenderPipeline,
    /// The opaque pipeline without `discard`, for the quads whose texture has
    /// no cutout texel (F2-2a).
    terrain_solid_pipeline: wgpu::RenderPipeline,
    /// The index buffer every chunk pass is drawn with (F2-2d).
    quad_indices: QuadIndices,
    /// Which layers of the bound block array need the alpha test.
    cutout: Vec<bool>,
    /// Bumped whenever `cutout` changes; chunk passes sorted with an older
    /// one are drawn whole with the alpha test until they are uploaded again.
    cutout_epoch: u32,
    /// The same for the built-in array, restored by `use_procedural_blocks`.
    procedural_cutout: Vec<bool>,
    /// Reused by `upload_mesh` to put the solid quads first.
    chunk_scratch: Vec<Vertex>,
    /// Cave culling (F2-3d): the sections the camera can see through the
    /// connectivity of the others.
    cave_graph: CaveGraph,
    cave_culling: bool,
    /// Bumped whenever a chunk mesh is added, replaced or removed.
    chunks_version: u64,
    /// What the visible lists were last built from: the view, the loaded meshes and
    /// the options, so a frame where none changed keeps them (perf 2, D1).
    visible_key: Option<(glam::Mat4, glam::DVec3, u64, i32, bool)>,
    /// Terrain pipeline for water, glass and leaves: no depth write, straight
    /// alpha blending. Drawn after every opaque chunk.
    terrain_transparent_pipeline: wgpu::RenderPipeline,
    crack_pipeline: wgpu::RenderPipeline,
    /// The crack quads of every block being dug, rebuilt by the client.
    crack_mesh: Option<GpuMeshPass>,
    moving_blocks: MovingBlocksPass,
    /// The camera of the first-person hand: eye-space vertices, a squeezed
    /// depth range, no fog.
    hand_camera_buffer: wgpu::Buffer,
    hand_camera_bind_group: wgpu::BindGroup,
    /// The vertices of the hand and its skin texture, if one is drawn.
    hand: BatchBuffer,
    terrain_bind_group: wgpu::BindGroup,
    /// The same array with the mip sampler, for the chunk passes only.
    chunk_bind_group: wgpu::BindGroup,
    /// Nearest texel, linear between the mip levels.
    mip_sampler: wgpu::Sampler,
    /// Mip levels below the base the next block atlas is uploaded with
    /// (`GameSettings.mipmapLevels`).
    mipmap_levels: u32,
    /// Layout of the terrain camera + texture binding, kept so
    /// [`Renderer::set_block_atlas`] can bind a replacement array without
    /// rebuilding the pipeline.
    terrain_layout: wgpu::BindGroupLayout,
    /// Reused per-frame pre-resolved draw commands for solid chunks (nearest-first).
    visible_solid: Vec<PreparedChunkDraw>,
    /// Reused per-frame pre-resolved draw commands for cutout chunks (nearest-first).
    visible_cutout: Vec<PreparedChunkDraw>,
    /// Reused per-frame pre-resolved draw commands for transparent chunks (farthest-first).
    visible_transparent: Vec<PreparedChunkDraw>,
    /// Scratch candidate list for sorting opaque chunks.
    opaque_candidates: Vec<OpaqueCandidate>,
    /// Scratch candidate list for sorting transparent chunks.
    transparent_candidates: Vec<TransparentCandidate>,
    /// Sky dome, sun, moon, stars and the End cube.
    sky: SkyRenderer,
    /// The clouds, drawn with the depth test after the opaque passes.
    clouds: CloudRenderer,
    /// Client ticks (with the fraction) that drive the cloud drift and the
    /// animation of the rain and snow.
    cloud_ticks: f64,
    /// The torch flicker of the lightmap, stepped once per client tick.
    torch_flicker: TorchFlicker,
    /// Rain and snow, drawn before the translucent terrain.
    weather: WeatherRenderer,
    particles: ParticleRenderer,
    particle_sprites: Vec<ParticleSprite>,
    /// Underwater, in-wall, fire, pumpkin, portal and vignette (P8-f).
    screen: ScreenRenderer,
    screen_overlays: ScreenOverlays,
    /// The world border walls, drawn after the translucent terrain.
    border: BorderRenderer,
    /// The title-screen panorama, drawn first when a view is set.
    panorama: PanoramaRenderer,
    /// World clock and dimension of the next frame.
    environment: RenderEnvironment,
    /// Nearest-neighbour sampler shared by the fallback and installed arrays.
    block_sampler: wgpu::Sampler,
    /// The built-in procedural block array, kept so
    /// [`Renderer::use_procedural_blocks`] can bind it again after an
    /// official atlas replaced it.
    procedural_blocks: wgpu::TextureView,
    /// The official block array and its layer size, kept so animated layers
    /// can be rewritten in place; `None` while the procedural array is bound.
    official_blocks: Option<(wgpu::Texture, u32, u32)>,
    /// The textures larger than the layers of the block array, kept at their
    /// size in a second array.
    hires: HiresBlocks,
    overlay_pipeline: wgpu::RenderPipeline,
    /// The name tags of the frame, drawn in the world pass.
    nametags: NametagRenderer,
    /// `overlay_pipeline` with smooth coverage instead of a threshold, for
    /// the brand font.
    brand_pipeline: wgpu::RenderPipeline,
    /// Pipeline for GUI texture quads (RGBA, no coverage threshold).
    gui_overlay_pipeline: wgpu::RenderPipeline,
    /// `gui_overlay_pipeline` with the inverting blend of the crosshair.
    gui_inverted_pipeline: wgpu::RenderPipeline,
    /// Layout and sampler shared by every font texture, so
    /// [`Renderer::set_font_atlas`] can bind a replacement texture without
    /// recreating the overlay pipeline.
    overlay_layout: wgpu::BindGroupLayout,
    /// Layout for the RGBA GUI atlas, same entries as `overlay_layout` but
    /// paired with `gui_overlay_pipeline`.
    gui_overlay_layout: wgpu::BindGroupLayout,
    font_sampler: wgpu::Sampler,
    /// Linear sampler of the brand font: its glyphs are antialiased outlines,
    /// not pixel art.
    brand_sampler: wgpu::Sampler,
    /// Brand font texture binding; `None` until
    /// [`Renderer::set_brand_font`] is called.
    brand_bind_group: Option<wgpu::BindGroup>,
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
    /// Item icons rendered through the terrain pipeline, drawn as UI quads.
    icon_atlas: IconAtlas,
    /// Terrain pipeline without culling, for icon rendering.
    icon_pipeline: wgpu::RenderPipeline,
    /// The icon camera over the current block texture array.
    icon_bind_group: wgpu::BindGroup,
    /// The icon camera for the entity pipeline (skull and banner icons).
    icon_entity_bind_group: wgpu::BindGroup,
    /// Binding that samples the icon atlas from the GUI overlay pipeline.
    icon_gui_bind_group: wgpu::BindGroup,
    /// The enchantment glint drawn over glinting icons.
    glint: GlintPass,
    /// The glint over world and hand item meshes.
    world_glint: WorldGlint,
    /// Reused geometry buffer of the icon being rendered.
    icon_mesh: ItemMesh,
    /// Dropped items and other world-space item geometry, rewritten per frame.
    world_items: DynamicMesh,
    /// The held item in camera space, drawn with the hand's depth range.
    hand_items: DynamicMesh,
    /// The hand camera over the current block texture array.
    hand_item_bind_group: wgpu::BindGroup,
    overlay_buffer: wgpu::Buffer,
    overlay_capacity: u64,
    offscreen: Option<OffscreenTarget>,
    chunks: ChunkMap<u32>,
    mesh_slots: Vec<Option<MeshSlot>>,
    free_mesh_slots: Vec<u32>,
    camera_bind_group: wgpu::BindGroup,
    entity_renderer: EntityRenderer,
    outline: OutlineRenderer,
    /// The blur of the game behind a menu, and whether a menu asks for it.
    scene_blur: SceneBlur,
    /// The motion blur of the world (the Motion Blur mod).
    motion_blur: MotionBlur,
    background_blur: bool,
    /// Every light level is full: the Fullbright mod.
    fullbright: bool,
    entity_batches: BatchBuffer,
    /// The glowing layers of the entities, drawn after them.
    additive_batches: BatchBuffer,
    /// The enchantment glint of worn armour, drawn over it at equal depth.
    glint_batches: BatchBuffer,
    /// Invisible entities a spectator sees, drawn translucent.
    ghost_batches: BatchBuffer,
    /// The elder guardian apparition, drawn translucent with depth.
    apparition_batches: BatchBuffer,
    /// The guardian beams, opaque and coloured by their tint.
    tinted_batches: BatchBuffer,
    /// The shadows under the entities.
    shadow_batches: BatchBuffer,
    /// Cached uniforms to avoid redundant `queue.write_buffer` transfers when uniforms are unchanged.
    last_camera_uniform: Option<CameraUniform>,
    last_local_uniform: Option<CameraUniform>,
    last_hand_uniform: Option<CameraUniform>,
    /// Draw call breakdown recorded by the most recent `draw` pass.
    last_draw_calls: std::cell::Cell<DrawCallStats>,
    /// Duration of the most recent swapchain presentation in microseconds.
    last_present_us: std::cell::Cell<u32>,
    /// Frame pacing and swapchain diagnostics tracker.
    frame_pacing: FramePacingTracker,
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
        umicraft_core::boot_log!("renderer_create_started", "width={width}, height={height}, format={format:?}");
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

    /// Sets the world clock and dimension used by the sky and fog passes.
    ///
    /// The default is midday in the overworld. Call once per frame before
    /// [`Renderer::render`] or [`Renderer::render_with_ui`]; the value is
    /// plain data and does not allocate.
    pub fn set_environment(&mut self, environment: RenderEnvironment) {
        self.environment = environment;
    }

    /// Replaces the sun, moon and End sky images (`textures/environment/`).
    ///
    /// Images are decoded by the caller from the user's installation. An
    /// absent or malformed image keeps the original generated fallback, so a
    /// missing jar never breaks the sky. Calling it again replaces the set.
    pub fn set_sky_textures(&mut self, textures: &SkyTextures) {
        self.sky.set_textures(&self.device, &self.queue, textures);
        self.clouds
            .set_texture(&self.device, &self.queue, textures.clouds.as_ref());
        self.set_weather_textures(textures);
        self.particles
            .set_sheet(&self.device, &self.queue, textures.particles.as_ref());
        self.screen.set_pictures(
            &self.device,
            &self.queue,
            Pictures {
                underwater: textures.underwater.as_ref(),
                pumpkin_blur: textures.pumpkin_blur.as_ref(),
                vignette: textures.vignette.as_ref(),
            },
        );
    }

    /// Replaces the rain and snow pictures (`textures/environment/rain.png`,
    /// `snow.png`) from the `rain` and `snow` fields of `textures`; an absent
    /// or malformed one keeps the original generated fallback.
    pub fn set_weather_textures(&mut self, textures: &SkyTextures) {
        self.weather.set_textures(
            &self.device,
            &self.queue,
            textures.rain.as_ref(),
            textures.snow.as_ref(),
        );
    }

    /// Sets the particles to draw this frame, already resolved by the client
    /// (position relative to the render origin, picture, tint, light). Call once per frame; the list is
    /// copied into a reused buffer.
    pub fn set_particles(&mut self, sprites: &[ParticleSprite]) {
        self.particle_sprites.clear();
        self.particle_sprites.extend_from_slice(sprites);
    }

    /// Sets the full-screen overlays to draw this frame (underwater, in a
    /// wall, fire, pumpkin, portal, vignette). Call once per frame.
    pub fn set_screen_overlays(&mut self, overlays: ScreenOverlays) {
        self.screen_overlays = overlays;
    }

    /// Sets the columns the rain and snow fall on, sampled by the game
    /// thread, and whether graphics are fancy (precipitation reaches 10
    /// blocks instead of 5). Call once per frame; it only stores the handle.
    pub fn set_precipitation(&mut self, field: Arc<PrecipitationField>, fancy: bool) {
        self.weather.set_field(field, fancy);
    }

    /// Chooses the cloud version (`GameSettings.clouds` and `fancyGraphics`).
    ///
    /// Call once per frame; it only stores the choice.
    pub fn set_cloud_mode(&mut self, mode: CloudMode) {
        self.clouds.set_mode(mode);
    }

    /// Replaces the cloud picture (`textures/environment/clouds.png`). A
    /// malformed image restores the original generated mask.
    pub fn set_cloud_texture(&mut self, image: &SkyImage) {
        self.clouds
            .set_texture(&self.device, &self.queue, Some(image));
    }

    /// Sets the cloud drift clock in client ticks, fraction included
    /// (`cloudTickCounter + partialTicks`).
    pub fn set_cloud_ticks(&mut self, ticks: f64) {
        self.cloud_ticks = ticks;
    }

    /// Sets the world border of the next frame (`None` hides it) and the
    /// clock in seconds that scrolls its texture every 3 s
    /// (`getSystemTime() % 3000L`). Call once per frame.
    pub fn set_world_border(&mut self, border: Option<BorderView>, clock_seconds: f64) {
        self.border.set_view(border, clock_seconds);
    }

    /// Replaces the border picture (`textures/misc/forcefield.png`). A
    /// malformed image restores the original generated pattern.
    pub fn set_border_texture(&mut self, image: &SkyImage) {
        self.border
            .set_texture(&self.device, &self.queue, Some(image));
    }

    /// Uploads the six panorama faces (`panorama_0` to `panorama_5`, square
    /// RGBA) and returns the ids the title screen hands back in its
    /// [`PanoramaView`]; `None` when a face is malformed. Call once at start.
    pub fn upload_panorama(&mut self, faces: &[SkyImage; 6]) -> Option<PanoramaTextures> {
        self.panorama.upload(&self.device, &self.queue, faces)
    }

    /// Sets the panorama of the next frame, drawn over the sky and under the
    /// UI. `None`, or a view of an older upload, shows none; the session must
    /// set `None` so the world is not covered.
    pub fn set_panorama(&mut self, view: Option<PanoramaView>) {
        self.panorama
            .set_view(&self.queue, view, (self.width, self.height));
    }

    /// Turns the title panorama's blur on (default) or off; off draws the
    /// single sharp pass. Applies from the next [`Self::set_panorama`].
    pub fn set_panorama_blur(&mut self, enabled: bool) {
        self.panorama.set_blur(enabled);
    }

    /// Replaces the break cracks drawn over the terrain.
    ///
    /// `vertices` and `indices` are the faces of the blocks being dug, with
    /// the layer of their destroy-stage texture; empty slices remove the
    /// cracks.
    pub fn set_block_cracks(&mut self, vertices: &[Vertex], indices: &[u32]) {
        self.crack_mesh = create_gpu_mesh_pass(&self.device, "block cracks", vertices, indices);
    }

    /// Replaces the blocks a piston is moving.
    ///
    /// `mesh` is rebuilt by the caller every frame in world coordinates; an
    /// empty mesh hides them. Its opaque faces are drawn with the chunks'
    /// terrain pipeline, its transparent faces with the blended one.
    pub fn set_moving_blocks(&mut self, mesh: &Mesh) {
        self.moving_blocks.update(&self.device, &self.queue, mesh);
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

    /// Switches the motion blur of the world on (`Some`) or off (`None`), or
    /// changes its version and strength (the Motion Blur mod). The window calls
    /// this every frame; the same settings cost nothing. While the camera stands
    /// still, while the blur is off and while a menu blurs the game, a frame costs
    /// nothing extra.
    pub fn set_motion_blur(&mut self, settings: Option<MotionBlurSettings>) {
        self.motion_blur.set_settings(&self.device, settings);
    }

    /// Tells the motion blur where the eye of the player is while the camera shows
    /// the player from outside (the third person), or `None` in the first person.
    /// The body of the player moves with the camera, not with the world, so a
    /// capsule around it is left unblurred. The window calls this every frame.
    pub fn set_motion_blur_body(&mut self, eye: Option<glam::DVec3>) {
        self.motion_blur.set_body(eye);
    }

    /// Lights the world at full strength whatever the light levels (the Fullbright
    /// mod): every entry of the lightmap is white. The shade of the faces and the
    /// ambient occlusion stay, so the world still has volume. The window calls this
    /// every frame.
    pub fn set_fullbright(&mut self, on: bool) {
        self.fullbright = on;
    }

    /// Tells the motion blur whether the hand in front of the camera moves this
    /// frame (a swing, an item being raised): the hand is blurred, and a still
    /// camera is blurred too while it moves. The window calls this every frame.
    pub fn set_motion_blur_hand_moving(&mut self, moving: bool) {
        self.motion_blur.set_hand_moving(moving);
    }

    /// Blurs the game behind the HUD and the UI (the mods menu) by `strength`,
    /// `0.0` (sharp) to `1.0` (fully blurred); a menu raises it over its
    /// opening to fade the blur in. The window calls this every frame; a frame
    /// with any blur costs two extra full-screen copies and four
    /// half-resolution passes, a sharp one costs nothing.
    pub fn set_background_blur(&mut self, strength: f32) {
        self.background_blur = strength > 0.0;
        self.scene_blur.set_strength(&self.queue, strength);
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
    ///
    /// `Item` commands draw from the icon atlas; an icon not rendered yet is
    /// built by `icons` and rendered first (once per key), so a steady-state
    /// frame renders no item geometry.
    pub fn render_with_ui(
        &mut self,
        camera: &Camera,
        overlay: &Overlay,
        draw_list: &DrawList,
        icons: &mut dyn IconProvider,
    ) -> Result<(), RenderError> {
        self.prepare_icons(draw_list, icons);
        let atlas = &self.icon_atlas;
        self.ui_pass
            .rebuild_with_icons(draw_list, &self.ui_atlas, &|key| atlas.uv(key));
        self.render_frame(camera, overlay)
    }

    /// Replaces the name tags drawn in the world pass of the next frames; an
    /// empty slice hides them. `size` is the window in pixels.
    pub fn set_nametags(&mut self, tags: &[Nametag], size: (u32, u32)) {
        self.nametags
            .set(&self.device, &self.queue, tags, size, &self.ui_atlas);
    }

    /// Whether the icon atlas holds the icon `key`.
    #[must_use]
    pub fn has_icon(&self, key: u64) -> bool {
        self.icon_atlas.contains(key)
    }

    /// Forgets the icon `key`, so that it is built again when asked for.
    pub fn forget_icon(&mut self, key: u64) {
        self.icon_atlas.forget(key);
    }

    /// Renders an icon drawn by a tile-entity model (a skull or a banner) into
    /// the icon atlas under `key`.
    ///
    /// `vertices` are posed into the 16x16 GUI box like an [`ItemMesh`]
    /// (`umicraft_model::item::pose::gui_pose`) and sample `texture`, an entity
    /// texture bind group. Nothing is drawn for an empty list. The icon uses
    /// the icon variant of the entity pipeline, lit at full light by the two
    /// fixed GUI lights (`gui_shade` in the entity shader).
    pub fn render_entity_icon(
        &mut self,
        key: u64,
        vertices: &[EntityVertex],
        texture: &wgpu::BindGroup,
    ) {
        if vertices.is_empty() {
            return;
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("entity icon vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let batch = [(buffer.slice(..), vertices.len() as u32, texture)];
        let (entities, camera) = (&self.entity_renderer, &self.icon_entity_bind_group);
        self.icon_atlas
            .render_with(&self.device, &self.queue, key, |pass| {
                entities.render_icon(pass, camera, batch);
            });
    }

    /// Redraws the portrait, the large icon of the inventory screen that
    /// shows the player, under [`crate::PORTRAIT_KEY`].
    ///
    /// Each batch is a vertex list and the entity texture it samples. The
    /// vertices are posed in the 16x16 GUI box like a
    /// [`Renderer::render_entity_icon`] icon, but the box fills the whole
    /// [`crate::PORTRAIT_SIZE`] square, so the caller scales the model to fit.
    /// The pipeline is the icon one: the two GUI lights at full light.
    pub fn render_portrait(
        &mut self,
        batches: &[(&[EntityVertex], &wgpu::BindGroup)],
        items: &ItemMesh,
    ) {
        let buffers: Vec<(wgpu::Buffer, u32, &wgpu::BindGroup)> = batches
            .iter()
            .filter(|(vertices, _)| !vertices.is_empty())
            .map(|&(vertices, texture)| {
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("portrait vertices"),
                        contents: bytemuck::cast_slice(vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                (buffer, vertices.len() as u32, texture)
            })
            .collect();
        // The item in the hand, already in the icon box, drawn with the pipeline
        // of the item icons (same camera, no culling).
        let held = create_gpu_mesh_pass(
            &self.device,
            "portrait item",
            &items.vertices,
            &items.indices,
        );
        let (entities, camera) = (&self.entity_renderer, &self.icon_entity_bind_group);
        let (icon_pipeline, icon_group) = (&self.icon_pipeline, &self.icon_bind_group);
        self.icon_atlas
            .render_portrait(&self.device, &self.queue, |pass| {
                entities.render_icon(
                    pass,
                    camera,
                    buffers.iter().map(|(b, n, t)| (b.slice(..), *n, *t)),
                );
                if let Some(held) = &held {
                    pass.set_pipeline(icon_pipeline);
                    pass.set_bind_group(0, icon_group, &[]);
                    draw_gpu_pass(pass, held);
                }
            });
    }

    /// Replaces the world-space item geometry drawn after the entities
    /// (dropped items, frames), already positioned relative to the render
    /// origin of the camera (F2-1b).
    pub fn set_world_items(&mut self, mesh: &ItemMesh) {
        self.world_items.update(&self.device, &self.queue, mesh);
    }

    /// Replaces the first-person held item, in camera space like the hand.
    pub fn set_hand_items(&mut self, mesh: &ItemMesh) {
        self.hand_items.update(&self.device, &self.queue, mesh);
    }

    /// Sets the world item geometry that gets the enchantment glint: the
    /// enchanted subset of the mesh given to [`Renderer::set_world_items`].
    /// An empty mesh clears it; nothing shows before [`Renderer::set_glint_texture`].
    pub fn set_world_glint(&mut self, mesh: &ItemMesh) {
        self.world_glint
            .update_world(&self.device, &self.queue, mesh);
    }

    /// Sets the first-person item geometry that gets the enchantment glint
    /// (the enchanted held item of [`Renderer::set_hand_items`]).
    pub fn set_hand_glint(&mut self, mesh: &ItemMesh) {
        self.world_glint
            .update_hand(&self.device, &self.queue, mesh);
    }

    /// Renders the icons `draw_list` asks for that the atlas lacks.
    fn prepare_icons(&mut self, draw_list: &DrawList, icons: &mut dyn IconProvider) {
        for command in draw_list.commands() {
            let DrawCommand::Item { key, .. } = command else {
                continue;
            };
            if self.icon_atlas.contains(*key) {
                continue;
            }
            self.icon_mesh.clear();
            if icons.build_icon(*key, &mut self.icon_mesh) {
                self.icon_atlas.render(
                    &self.device,
                    &self.queue,
                    &self.icon_pipeline,
                    &self.icon_bind_group,
                    *key,
                    &self.icon_mesh,
                );
            }
        }
    }

    fn render_frame(&mut self, camera: &Camera, overlay: &Overlay) -> Result<(), RenderError> {
        umicraft_core::profile_scope!("render_frame");
        // One SkyState per frame: the clear colour, the fog uniform and the
        // sky pass must agree even if W3's model changes between calls.
        let sky_state = SkyState::at_looking(self.environment, camera.forward().to_array());
        let flicker = self.torch_flicker.at(self.cloud_ticks as u64);
        self.write_camera(camera, &sky_state, flicker);
        self.sky
            .update(&self.queue, camera, self.environment, &sky_state);
        self.clouds.update(
            &self.queue,
            camera,
            self.environment,
            &sky_state,
            self.cloud_ticks,
        );
        self.weather
            .update(&self.queue, camera, self.environment, self.cloud_ticks);
        self.particles
            .update(&self.queue, camera, &self.particle_sprites);
        self.screen
            .update(&self.queue, &self.screen_overlays, camera);
        // The reference's reach is `renderDistanceChunks * 16`.
        self.border.update(
            &self.queue,
            camera,
            f64::from(self.environment.view_distance) * 16.0,
        );
        self.upload_overlay(overlay);
        self.prepare_visible_chunks(camera);
        // The motion blur of this frame, unless a menu already blurs the game.
        self.motion_blur.prepare(
            &self.device,
            &self.queue,
            camera,
            (self.scene_blur.scene_view(), &self.depth_view),
            (self.width, self.height),
            !self.background_blur,
        );
        if self.surface.is_some() {
            self.render_window(overlay, &sky_state)
        } else {
            self.render_offscreen(overlay, &sky_state)
        }
    }

#[inline(always)]
fn collect_chunk_runs(pass: &ChunkPass, mask: u16, group: Group) -> ChunkRuns {
    let mut runs = ChunkRuns::empty();
    pass.visible_runs(mask, group, |r| runs.push(r));
    runs
}

    /// Rebuilds the reused per-frame draw lists from the chunks that are
    /// inside the render distance and the view frustum.
    ///
    /// The server may keep sending or keeping columns beyond the client's
    /// render distance, and the GPU would still shade every one of them, so
    /// the cull happens here. The transparent list is sorted farthest first;
    /// both lists keep their capacity across frames.
    fn prepare_visible_chunks(&mut self, camera: &Camera) {
        umicraft_core::profile_scope!("prepare_visible_chunks");
        // Boxes are made relative to the render origin in f64, so the frustum
        // planes and the boxes stay precise far from the world origin (A5).
        let origin = camera.render_origin();
        let view_projection = camera.view_projection_from(origin);
        let eye = camera.eye();
        let reach = i32::from(self.environment.view_distance);
        let key = (
            view_projection,
            eye,
            self.chunks_version,
            reach,
            self.cave_culling,
        );
        // A still camera over unchanged chunks has the lists it had.
        if self.visible_key == Some(key) {
            return;
        }
        self.visible_key = Some(key);
        let frustum = Frustum::from_view_projection(view_projection);
        let eye_chunk = ChunkPos::new((eye.x / 16.0).floor() as i32, (eye.z / 16.0).floor() as i32);
        self.visible_solid.clear();
        self.visible_cutout.clear();
        self.visible_transparent.clear();
        self.opaque_candidates.clear();
        self.transparent_candidates.clear();

        if self.cave_culling {
            let slots = &self.mesh_slots;
            let chunk_map = &self.chunks;
            let cave_reach = reach.min(24);
            self.cave_graph
                .update((eye.x, eye.y, eye.z), cave_reach, self.chunks_version, |pos| {
                    chunk_map.get(&pos).and_then(|&slot| {
                        slots.get(slot as usize).and_then(Option::as_ref).map(|s| &s.mesh.connectivity)
                    })
                });
        }
        let cave = self.cave_culling.then_some(&self.cave_graph);
        let cave_reach = reach.min(24);
        for (slot_idx, slot_entry) in self.mesh_slots.iter().enumerate() {
            let Some(entry) = slot_entry.as_ref() else { continue; };
            let pos = entry.pos;
            let mesh = &entry.mesh;
            let dx = (pos.x - eye_chunk.x).abs();
            let dz = (pos.z - eye_chunk.z).abs();
            if dx > reach || dz > reach {
                continue;
            }
            let cave_mask = match cave {
                Some(c) => {
                    if dx <= cave_reach && dz <= cave_reach {
                        let m = c.mask(pos);
                        if m == 0 {
                            continue;
                        }
                        m
                    } else {
                        0xFFFF
                    }
                }
                None => 0xFFFF,
            };
            let sections = sections_in_frustum(pos, &frustum, origin) & cave_mask;
            if sections == 0 {
                continue;
            }
            let distance_sq = squared_distance_to_chunk(pos, eye);
            if let Some(opaque) = &mesh.opaque {
                let is_split = opaque.epoch == self.cutout_epoch;
                let has_solid = is_split && opaque.solid_quads > 0;
                let has_cutout = !is_split || opaque.solid_quads < opaque.quads;
                if has_solid || has_cutout {
                    self.opaque_candidates.push(OpaqueCandidate {
                        slot: slot_idx as u32,
                        pos,
                        distance_sq,
                        sections,
                        has_cutout,
                    });
                }
            }
            if mesh.transparent.is_some() {
                self.transparent_candidates.push(TransparentCandidate {
                    slot: slot_idx as u32,
                    pos,
                    distance_sq,
                    sections,
                });
            }
        }

        self.opaque_candidates.sort_unstable_by(|a, b| {
            a.distance_sq
                .total_cmp(&b.distance_sq)
                .then_with(|| a.pos.cmp(&b.pos))
        });
        for c in &self.opaque_candidates {
            let Some(slot) = self.mesh_slots.get(c.slot as usize).and_then(Option::as_ref) else { continue; };
            let Some(opaque) = &slot.mesh.opaque else { continue; };
            if opaque.epoch == self.cutout_epoch {
                if !c.has_cutout {
                    let runs = if c.sections == 0xFFFF {
                        let mut r = ChunkRuns::empty();
                        r.push(0..opaque.quads * 6);
                        r
                    } else {
                        Self::collect_chunk_runs(opaque, c.sections, Group::Solid)
                    };
                    if runs.count > 0 {
                        self.visible_solid.push(PreparedChunkDraw { slot: c.slot, runs });
                    }
                } else {
                    let mut runs = ChunkRuns::empty();
                    if c.sections == 0xFFFF {
                        runs.push(0..opaque.quads * 6);
                    } else {
                        let solid_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Solid);
                        let cutout_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Cutout);
                        for r in solid_runs.as_slice() {
                            runs.push(r.clone());
                        }
                        for r in cutout_runs.as_slice() {
                            runs.push(r.clone());
                        }
                    }
                    if runs.count > 0 {
                        self.visible_cutout.push(PreparedChunkDraw { slot: c.slot, runs });
                    }
                }
            } else {
                let mut runs = ChunkRuns::empty();
                if c.sections == 0xFFFF {
                    runs.push(0..opaque.quads * 6);
                } else {
                    let solid_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Solid);
                    let cutout_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Cutout);
                    for r in solid_runs.as_slice() {
                        runs.push(r.clone());
                    }
                    for r in cutout_runs.as_slice() {
                        runs.push(r.clone());
                    }
                }
                if runs.count > 0 {
                    self.visible_cutout.push(PreparedChunkDraw { slot: c.slot, runs });
                }
            }
        }

        self.transparent_candidates.sort_unstable_by(|a, b| {
            b.distance_sq
                .total_cmp(&a.distance_sq)
                .then_with(|| a.pos.cmp(&b.pos))
        });
        for c in &self.transparent_candidates {
            let Some(slot) = self.mesh_slots.get(c.slot as usize).and_then(Option::as_ref) else { continue; };
            let Some(transparent) = &slot.mesh.transparent else { continue; };
            let runs = if c.sections == 0xFFFF {
                let mut r = ChunkRuns::empty();
                r.push(0..transparent.quads * 6);
                r
            } else {
                Self::collect_chunk_runs(transparent, c.sections, Group::Solid)
            };
            if runs.count > 0 {
                self.visible_transparent.push(PreparedChunkDraw { slot: c.slot, runs });
            }
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
        self.cutout = cutout_layers(pixels, width as usize * height as usize * 4);
        self.cutout_epoch += 1;
        self.official_blocks = Some((texture, width, height));
        Ok(())
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
        self.cutout.clone_from(&self.procedural_cutout);
        self.cutout_epoch += 1;
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

    /// Replaces the GPU mesh for `pos`, or removes it when both passes are
    /// empty.
    ///
    /// The opaque and transparent halves are uploaded into separate vertex and
    /// index buffers so the renderer can submit every opaque chunk before the
    /// sorted transparent pass. A half with no faces gets no buffer.
    pub fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        umicraft_core::profile_scope!("upload_mesh");
        self.chunks_version += 1;
        if mesh.is_empty() {
            if let Some(slot) = self.chunks.remove(&pos) {
                self.mesh_slots[slot as usize] = None;
                self.free_mesh_slots.push(slot);
            }
            return;
        }
        // The spans of each pass, when the mesher produced them.
        let has_sections = mesh.sections.len() == SECTIONS;
        let opaque_spans: [VertexSpan; SECTIONS] = std::array::from_fn(|i| {
            mesh.sections
                .get(i)
                .map_or_else(Default::default, |s| s.opaque)
        });
        let transparent_spans: [VertexSpan; SECTIONS] = std::array::from_fn(|i| {
            mesh.sections
                .get(i)
                .map_or_else(Default::default, |s| s.transparent)
        });
        let opaque = create_chunk_pass(
            &self.device,
            "chunk opaque",
            &mesh.vertices,
            has_sections.then_some(&opaque_spans[..]),
            Some((&self.cutout, self.cutout_epoch)),
            &mut self.chunk_scratch,
        );
        let transparent = create_chunk_pass(
            &self.device,
            "chunk transparent",
            &mesh.transparent_vertices,
            has_sections.then_some(&transparent_spans[..]),
            None,
            &mut self.chunk_scratch,
        );
        // A pass without a whole quad has nothing to draw. If neither half
        // produced a buffer, drop any previous mesh instead of keeping an
        // empty entry.
        if opaque.is_none() && transparent.is_none() {
            if let Some(slot) = self.chunks.remove(&pos) {
                self.mesh_slots[slot as usize] = None;
                self.free_mesh_slots.push(slot);
            }
            return;
        }
        let most_quads = [&opaque, &transparent]
            .into_iter()
            .flatten()
            .map(|pass| pass.quads)
            .max()
            .unwrap_or(0);
        self.quad_indices.ensure(&self.device, most_quads);
        let connectivity = std::array::from_fn(|i| {
            mesh.sections
                .get(i)
                .map_or(Connectivity::OPEN, |section| section.connectivity)
        });
        let mesh = GpuMesh {
            opaque,
            transparent,
            connectivity,
        };
        match self.chunks.entry(pos) {
            Entry::Occupied(entry) => {
                let slot = *entry.get();
                self.mesh_slots[slot as usize] = Some(MeshSlot { pos, mesh });
            }
            Entry::Vacant(entry) => {
                let slot = match self.free_mesh_slots.pop() {
                    Some(s) => {
                        self.mesh_slots[s as usize] = Some(MeshSlot { pos, mesh });
                        s
                    }
                    None => {
                        let s = self.mesh_slots.len() as u32;
                        self.mesh_slots.push(Some(MeshSlot { pos, mesh }));
                        s
                    }
                };
                entry.insert(slot);
            }
        }
    }

    /// Removes the GPU mesh for one chunk column.
    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks_version += 1;
        if let Some(slot) = self.chunks.remove(&pos) {
            self.mesh_slots[slot as usize] = None;
            self.free_mesh_slots.push(slot);
        }
    }

    /// Removes every uploaded chunk mesh (e.g. after a dimension change).
    pub fn clear_meshes(&mut self) {
        self.chunks_version += 1;
        self.chunks.clear();
        self.mesh_slots.clear();
        self.free_mesh_slots.clear();
        self.visible_solid.clear();
        self.visible_cutout.clear();
        self.visible_transparent.clear();
        self.visible_key = None;
    }

    /// Turns the cave culling of the terrain sections on or off (on by
    /// default). Off draws every section in the frustum.
    pub fn set_cave_culling(&mut self, enabled: bool) {
        self.cave_culling = enabled;
    }

    /// The GPU adapter the renderer runs on, for the F3 screen.
    #[must_use]
    pub fn adapter(&self) -> &AdapterSummary {
        &self.adapter
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
        self.scene_blur
            .resize(&self.device, &self.queue, (width, height));
        self.motion_blur.invalidate();
        self.ui_pass.resize(width, height);
        if self.offscreen.is_some() {
            self.offscreen = Some(create_offscreen(&self.device, width, height));
        }
    }

    /// Turns vertical sync on or off (`GameSettings.enableVsync`). The
    /// surface is only reconfigured when the mode changes, so the window can
    /// call this every frame. Offscreen renderers ignore it.
    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = present_mode(vsync);
        let latency = if vsync {
            FRAME_LATENCY_VSYNC
        } else {
            FRAME_LATENCY_UNCONSTRAINED
        };
        let (Some(config), Some(surface)) = (self.surface_config.as_mut(), self.surface.as_ref())
        else {
            return;
        };
        if config.present_mode != mode || config.desired_maximum_frame_latency != latency {
            config.present_mode = mode;
            config.desired_maximum_frame_latency = latency;
            surface.configure(&self.device, config);
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
        self.entity_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the glowing entity layers (eyes, charge, aura, glint) to be
    /// drawn after the entities. The `light` of their vertices is the tint
    /// (bits 0..=23, `0xBBGGRR`) and bit 24 squares the colour.
    pub fn upload_additive_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.additive_batches
            .set(&self.device, &self.queue, batches);
    }

    /// Uploads the shadows under the entities: squares whose `light` is the
    /// opacity in its low byte.
    pub fn upload_shadow_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.shadow_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the invisible entities a spectator sees (their model, drawn at
    /// 15 % opacity without writing depth).
    pub fn upload_ghost_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.ghost_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the guardian beams: opaque, unlit vertices whose light word
    /// is their red, green and blue.
    pub fn upload_tinted_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.tinted_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the elder guardian apparition (its model, translucent by the
    /// opacity bits of its vertices, writing depth).
    pub fn upload_apparition_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.apparition_batches
            .set(&self.device, &self.queue, batches);
    }

    /// Uploads the enchantment glint of worn armour: the same vertices as the
    /// armour, drawn only where the armour wrote its depth.
    pub fn upload_glint_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.glint_batches.set(&self.device, &self.queue, batches);
    }

    /// Sets the first-person hand for the following frames.
    ///
    /// `vertices` are in camera space (the camera at the origin looking down
    /// −Z, the view bobbing already applied) and use the skin texture of
    /// `bind_group`; an empty slice hides the hand. It is drawn on top of the
    /// world and under the HUD.
    pub fn set_hand(&mut self, vertices: &[EntityVertex], bind_group: &wgpu::BindGroup) {
        self.set_hand_batches(&[(vertices, bind_group)]);
    }

    /// Sets the first-person hand as several batches, drawn in order (the
    /// arms, then a held map with its background, pixels and markers).
    /// Empty batches are skipped.
    pub fn set_hand_batches(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.hand.set(&self.device, &self.queue, batches);
    }

    /// Hides the first-person hand.
    pub fn clear_hand(&mut self) {
        self.hand.clear();
    }

    /// Clears any queued entity batches.
    pub fn clear_entities(&mut self) {
        self.entity_batches.clear();
        self.additive_batches.clear();
        self.glint_batches.clear();
        self.ghost_batches.clear();
        self.apparition_batches.clear();
        self.tinted_batches.clear();
        self.shadow_batches.clear();
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

    /// Creates a repeating entity texture for the glowing layers, whose
    /// coordinates scroll past the edge.
    #[must_use]
    pub fn create_repeating_entity_texture(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> crate::entity_pass::EntityTexture {
        self.entity_renderer.create_repeating_texture(
            &self.device,
            &self.queue,
            rgba,
            width,
            height,
        )
    }

    /// Reads the offscreen colour target as tightly packed RGBA8.
    ///
    /// Call [`Renderer::render`] or [`Renderer::render_with_ui`] first. The
    /// row padding required by WebGPU is removed before returning.
    pub fn read_pixels(&self) -> Result<Image, RenderError> {
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        capture::read_texture(
            &self.device,
            &self.queue,
            &target.texture,
            (self.width, self.height),
            false,
        )
    }

    /// Asks the windowed renderer to keep the next frame it presents; fetch
    /// it with [`Renderer::take_capture`]. The surface must allow copies
    /// (see [`Renderer::can_capture`]).
    pub fn request_capture(&mut self) {
        self.capture_requested = true;
    }

    /// Whether the window surface can be read back (`COPY_SRC` usage).
    #[must_use]
    pub fn can_capture(&self) -> bool {
        self.surface_config
            .as_ref()
            .is_some_and(|config| config.usage.contains(wgpu::TextureUsages::COPY_SRC))
    }

    /// The frame kept after [`Renderer::request_capture`], once it was
    /// presented; `None` while nothing is waiting.
    ///
    /// # Errors
    /// [`RenderError::Readback`] when the copy of the frame failed.
    pub fn take_capture(&mut self) -> Option<Result<Image, RenderError>> {
        self.captured.take()
    }

    /// Copies the frame about to be presented when a capture was requested.
    fn capture_frame(&mut self, texture: &wgpu::Texture) {
        if !std::mem::take(&mut self.capture_requested) || !self.can_capture() {
            return;
        }
        let Some(format) = self.surface_config.as_ref().map(|config| config.format) else {
            return;
        };
        let blue_first = matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        self.captured = Some(capture::read_texture(
            &self.device,
            &self.queue,
            texture,
            (self.width, self.height),
            blue_first,
        ));
    }

    fn write_camera(&mut self, camera: &Camera, sky: &SkyState, flicker: f32) {
        let [fog_start, fog_end, fog_density] = sky.fog.world.uniform();
        let origin = camera.render_origin();
        let uniform = CameraUniform {
            view_proj: camera.view_projection_from(origin).to_cols_array_2d(),
            camera_pos: camera.relative_eye().to_array(),
            sun_brightness: sky.sun_brightness,
            fog_color: sky.fog.color,
            fog_start,
            fog_end,
            gamma: self.environment.gamma,
            fog_density,
            _pad: 0.0,
            origin: origin.as_vec3().to_array(),
            _pad_origin: 0.0,
            lightmap: if self.fullbright {
                lightmap::unlit()
            } else {
                lightmap::colors(&LightmapParams::of(
                    &self.environment,
                    sky.sun_brightness,
                    flicker,
                ))
            },
        };
        let uniform_bytes = bytemuck::bytes_of(&uniform);
        if self
            .last_camera_uniform
            .as_ref()
            .map_or(true, |cached| bytemuck::bytes_of(cached) != uniform_bytes)
        {
            self.queue.write_buffer(&self.camera_buffer, 0, uniform_bytes);
            self.last_camera_uniform = Some(uniform);
        }

        // Same matrix and eye: the vertices of this one are already relative.
        let local = CameraUniform {
            origin: [0.0; 3],
            ..uniform
        };
        let local_bytes = bytemuck::bytes_of(&local);
        if self
            .last_local_uniform
            .as_ref()
            .map_or(true, |cached| bytemuck::bytes_of(cached) != local_bytes)
        {
            self.queue.write_buffer(&self.local_camera_buffer, 0, local_bytes);
            self.last_local_uniform = Some(local);
        }

        // The hand lives in camera space, far from any fog.
        let hand_uniform = CameraUniform {
            view_proj: camera.hand_projection().to_cols_array_2d(),
            camera_pos: [0.0; 3],
            fog_start: HAND_FOG_FREE,
            fog_end: HAND_FOG_FREE * 2.0,
            fog_density: 0.0,
            origin: [0.0; 3],
            ..uniform
        };
        let hand_bytes = bytemuck::bytes_of(&hand_uniform);
        if self
            .last_hand_uniform
            .as_ref()
            .map_or(true, |cached| bytemuck::bytes_of(cached) != hand_bytes)
        {
            self.queue.write_buffer(
                &self.hand_camera_buffer,
                0,
                hand_bytes,
            );
            self.last_hand_uniform = Some(hand_uniform);
        }
    }

    fn upload_overlay(&mut self, overlay: &Overlay) {
        let hud_bytes: &[u8] = bytemuck::cast_slice(overlay.vertices());
        let hud_len = hud_bytes.len();
        let ui_bytes: &[u8] = bytemuck::cast_slice(self.ui_pass.vertices());
        let ui_len = ui_bytes.len();
        let total = hud_len + ui_len;
        if total == 0 {
            return;
        }
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
        if let Some(size) = wgpu::BufferSize::new(total as u64) {
            if let Some(mut view) = self.queue.write_buffer_with(&self.overlay_buffer, 0, size) {
                if !hud_bytes.is_empty() {
                    view.slice(..hud_len).copy_from_slice(hud_bytes);
                }
                if !ui_bytes.is_empty() {
                    view.slice(hud_len..total).copy_from_slice(ui_bytes);
                }
            } else {
                if !hud_bytes.is_empty() {
                    self.queue.write_buffer(&self.overlay_buffer, 0, hud_bytes);
                }
                if !ui_bytes.is_empty() {
                    self.queue
                        .write_buffer(&self.overlay_buffer, hud_len as u64, ui_bytes);
                }
            }
        }
    }

    fn render_window(
        &mut self,
        overlay: &Overlay,
        sky_state: &SkyState,
    ) -> Result<(), RenderError> {
        let frame_num = umicraft_core::boot_log::frames_presented();
        if frame_num < 10 {
            umicraft_core::boot_log!("render_window_started", "frame={frame_num}");
        }
        umicraft_core::profile_scope!("render_window");
        let acquire_start = std::time::Instant::now();
        if frame_num < 10 {
            umicraft_core::boot_log!("surface_get_current_texture_started", "frame={frame_num}");
        }
        let frame = {
            let surface = self
                .surface
                .as_ref()
                .ok_or_else(|| RenderError::Surface("renderer has no surface".into()))?;
            surface.get_current_texture()
        };
        let acquire_us =
            acquire_start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        if frame_num < 10 {
            let status = match &frame {
                wgpu::CurrentSurfaceTexture::Success(_) => "Success",
                wgpu::CurrentSurfaceTexture::Suboptimal(_) => "Suboptimal",
                wgpu::CurrentSurfaceTexture::Outdated => "Outdated",
                wgpu::CurrentSurfaceTexture::Timeout => "Timeout",
                wgpu::CurrentSurfaceTexture::Occluded => "Occluded",
                wgpu::CurrentSurfaceTexture::Lost => "Lost",
                wgpu::CurrentSurfaceTexture::Validation => "Validation",
            };
            umicraft_core::boot_log!(
                "surface_get_current_texture_result",
                "frame={frame_num}, status={status}, acquire_us={acquire_us}"
            );
        }

        match frame {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                let encode_start = std::time::Instant::now();
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("frame encoder"),
                        });
                self.begin_gpu_timing();
                self.draw(&view, overlay, &mut encoder, sky_state);
                let encode_us =
                    encode_start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;

                let submit_start = std::time::Instant::now();
                self.submit_timed(encoder);
                let submit_us =
                    submit_start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;

                self.capture_frame(&frame.texture);
                if let Some(window) = &self.window {
                    window.pre_present_notify();
                }
                if frame_num < 10 {
                    umicraft_core::boot_log!("queue_present_started", "frame={frame_num}");
                }
                let present_start = std::time::Instant::now();
                self.queue.present(frame);
                let present_now = std::time::Instant::now();
                let present_us = present_now
                    .saturating_duration_since(present_start)
                    .as_micros()
                    .min(u128::from(u32::MAX)) as u32;
                self.last_present_us.set(present_us);
                let presented_count = umicraft_core::boot_log::log_frame_presented();
                if frame_num < 10 {
                    umicraft_core::boot_log!(
                        "queue_present_done",
                        "frame={frame_num}, present_us={present_us}, total_presented={presented_count}"
                    );
                }

                self.frame_pacing.record_present(
                    present_now,
                    acquire_us,
                    encode_us,
                    submit_us,
                    present_us,
                );
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
                self.frame_pacing.suboptimal_count += 1;
                self.reconfigure_surface()?;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.frame_pacing.outdated_count += 1;
                self.reconfigure_surface()?;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.frame_pacing.timeout_count += 1;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.frame_pacing.occluded_count += 1;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.frame_pacing.lost_count += 1;
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
        self.begin_gpu_timing();
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("offscreen encoder"),
            });
        self.draw(&target.view, overlay, &mut encoder, sky_state);
        self.submit_timed(encoder);
        self.last_present_us.set(0);
        Ok(())
    }

    fn begin_gpu_timing(&mut self) {
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.begin_frame();
        }
    }

    /// Submits the frame and collects the finished GPU timings.
    fn submit_timed(&mut self, mut encoder: wgpu::CommandEncoder) {
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.resolve(&mut encoder);
        }
        self.queue.submit(Some(encoder.finish()));
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.after_submit(&self.device);
        }
    }

    /// The first-person hand and the item in it: camera-space vertices under a
    /// squeezed depth range, so the hand always wins the depth test.
    fn draw_hand(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.hand.is_empty() {
            self.entity_renderer
                .render(pass, &self.hand_camera_bind_group, self.hand.iter());
        }

        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.hand_item_bind_group, &[]);
        self.hand_items.draw(pass);
        self.world_glint.draw_hand(pass, &self.hand_item_bind_group);
    }

    fn stamp(&self, pass: &mut wgpu::RenderPass<'_>, index: u32) {
        if let Some(timer) = &self.gpu_timer {
            timer.stamp(pass, index);
        }
    }

    /// Blocks until the GPU finished every submitted frame, for benchmarks
    /// that time a whole frame.
    pub fn wait_idle(&self) {
        // A failed poll means the device is lost; the next draw reports it.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// The GPU time of the newest finished frame per group of draws, two or
    /// three frames old. `None` when the adapter has no timestamp queries or
    /// no frame has come back yet.
    #[must_use]
    pub fn gpu_timings(&self) -> Option<GpuTimings> {
        self.gpu_timer.as_ref().and_then(GpuTimer::latest)
    }

    /// Whether the adapter can time the GPU (F3 shows "n/a" otherwise).
    #[must_use]
    pub fn gpu_timing_supported(&self) -> bool {
        self.gpu_timer.is_some()
    }

    /// Returns the draw call breakdown recorded during the last rendered frame.
    #[must_use]
    pub fn draw_calls(&self) -> DrawCallStats {
        self.last_draw_calls.get()
    }

    /// Number of active weather vertices rendered in the last frame.
    #[must_use]
    pub fn weather_vertex_count(&self) -> usize {
        self.weather.vertex_count()
    }

    /// Returns the presentation duration of the last rendered frame in microseconds.
    #[must_use]
    pub fn presentation_time_us(&self) -> u32 {
        self.last_present_us.get()
    }

    /// Detailed configuration of the current WGPU presentation surface.
    #[must_use]
    pub fn surface_info(&self) -> Option<SurfaceInfo> {
        let config = self.surface_config.as_ref()?;
        Some(SurfaceInfo {
            backend: self.adapter.backend_type,
            backend_name: self.adapter.backend.clone(),
            adapter_name: self.adapter.name.clone(),
            adapter_vendor: self.adapter.vendor.clone(),
            format: config.format,
            present_mode: config.present_mode,
            desired_maximum_frame_latency: config.desired_maximum_frame_latency,
            width: config.width,
            height: config.height,
            usages: config.usage,
        })
    }

    /// Rolling frame presentation pacing and swapchain metrics.
    #[must_use]
    pub fn frame_pacing_stats(&self) -> FramePacingStats {
        self.frame_pacing.stats()
    }

    /// Estimates the GPU memory (VRAM) currently held by this renderer's
    /// chunk buffers, dynamic vertex/uniform buffers and textures.
    #[must_use]
    pub fn vram_stats(&self) -> VramStats {
        let mut chunk_buffers = self.quad_indices.byte_size();
        for slot in self.mesh_slots.iter().flatten() {
            let mesh = &slot.mesh;
            if let Some(opaque) = &mesh.opaque {
                chunk_buffers += opaque.vertex_buffer.size();
            }
            if let Some(transparent) = &mesh.transparent {
                chunk_buffers += transparent.vertex_buffer.size();
            }
        }
        let dynamic_buffers = self.overlay_capacity
            + self.camera_buffer.size()
            + self.local_camera_buffer.size()
            + self.hand_camera_buffer.size()
            + self.entity_batches.capacity_bytes()
            + self.additive_batches.capacity_bytes()
            + self.glint_batches.capacity_bytes()
            + self.ghost_batches.capacity_bytes()
            + self.shadow_batches.capacity_bytes()
            + self.hand.capacity_bytes();
        let pixel_count = u64::from(self.width) * u64::from(self.height);
        // Depth buffer (Depth32Float = 4B/px) + scene blur target (4B/px) + optional offscreen (4B/px).
        let target_bytes = pixel_count * if self.offscreen.is_some() { 12 } else { 8 };
        let atlas_bytes = match &self.official_blocks {
            Some((tex, w, h)) => {
                let base = u64::from(*w) * u64::from(*h) * 4 * u64::from(tex.depth_or_array_layers());
                // Mip chain adds at most ~1/3 over base.
                if tex.mip_level_count() > 1 {
                    base + base / 3
                } else {
                    base
                }
            }
            None => u64::from(TEXTURE_SIZE) * u64::from(TEXTURE_SIZE) * 4 * u64::from(TEXTURE_LAYERS),
        };
        let font_bytes = u64::from(self.ui_atlas.width) * u64::from(self.ui_atlas.height);
        let icon_bytes = u64::from(crate::items::ICON_ATLAS_SIZE)
            * u64::from(crate::items::ICON_ATLAS_SIZE)
            * 8; // colour + depth
        VramStats {
            chunk_buffers,
            dynamic_buffers,
            textures: target_bytes + atlas_bytes + font_bytes + icon_bytes,
        }
    }

    fn draw(
        &self,
        view: &wgpu::TextureView,
        overlay: &Overlay,
        encoder: &mut wgpu::CommandEncoder,
        sky_state: &SkyState,
    ) {
        self.panorama.prepare(encoder);
        // The reference clears to the fog colour; the dome fades into it.
        let clear = sky_state.fog.color;
        // With a menu that blurs the game, the world goes to a texture first.
        let blur = self.background_blur;
        // With a motion blur, the world goes to the texture too, and the hand and
        // the HUD are drawn over the blurred picture, sharp.
        let motion = self.motion_blur.is_active();
        let world_view = if blur || motion {
            self.scene_blur.scene_view()
        } else {
            view
        };
        let color_attachment = Some(wgpu::RenderPassColorAttachment {
            view: world_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(clear[0]),
                    g: f64::from(clear[1]),
                    b: f64::from(clear[2]),
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
        self.stamp(&mut pass, 0);
        let mut draw_stats = DrawCallStats {
            sky: 1,
            ..DrawCallStats::default()
        };
        self.sky.draw(&mut pass);
        self.panorama.draw(&mut pass);
        self.stamp(&mut pass, 1);

        // Opaque terrain, nearest chunk first: the SOLID quads (no alpha
        // test, early depth rejection) and then the CUTOUT ones, which
        // discard texels. Both write depth and do not depend on the order.
        self.quad_indices.bind(&mut pass);
        pass.set_bind_group(0, &self.chunk_bind_group, &[]);
        if !self.visible_solid.is_empty() {
            pass.set_pipeline(&self.terrain_solid_pipeline);
            draw_stats.opaque += self.draw_visible_opaque(&mut pass, true);
        }
        if !self.visible_cutout.is_empty() {
            pass.set_pipeline(&self.terrain_pipeline);
            draw_stats.opaque += self.draw_visible_opaque(&mut pass, false);
        }
        self.moving_blocks.draw_opaque(&mut pass);
        self.stamp(&mut pass, 2);

        // The shadows lie on the ground, drawn before the entities.
        if !self.shadow_batches.is_empty() {
            draw_stats.entities += self.shadow_batches.batch_count();
            let batches = self.shadow_batches.iter();
            self.entity_renderer
                .render_shadows(&mut pass, &self.local_camera_bind_group, batches);
        }

        // Entities: depth write on, cutout discard, rendered between opaque and transparent passes.
        if !self.entity_batches.is_empty() {
            draw_stats.entities += self.entity_batches.batch_count();
            let batches = self.entity_batches.iter();
            self.entity_renderer
                .render(&mut pass, &self.local_camera_bind_group, batches);
        }

        // The guardian beams, opaque like the entities they join.
        if !self.tinted_batches.is_empty() {
            let batches = self.tinted_batches.iter();
            self.entity_renderer
                .render_tinted(&mut pass, &self.local_camera_bind_group, batches);
        }

        // The invisible entities a spectator sees, over the opaque ones.
        if !self.ghost_batches.is_empty() {
            draw_stats.entities += self.ghost_batches.batch_count();
            let batches = self.ghost_batches.iter();
            self.entity_renderer
                .render_ghosts(&mut pass, &self.local_camera_bind_group, batches);
        }

        // The elder guardian apparition, a particle drawn after the entities.
        if !self.apparition_batches.is_empty() {
            let batches = self.apparition_batches.iter();
            self.entity_renderer.render_apparition(
                &mut pass,
                &self.local_camera_bind_group,
                batches,
            );
        }

        // The glow of the entities is added over them, without writing depth.
        if !self.additive_batches.is_empty() {
            draw_stats.entities += self.additive_batches.batch_count();
            let batches = self.additive_batches.iter();
            self.entity_renderer
                .render_additive(&mut pass, &self.local_camera_bind_group, batches);
        }

        if !self.glint_batches.is_empty() {
            draw_stats.entities += self.glint_batches.batch_count();
            let batches = self.glint_batches.iter();
            self.entity_renderer
                .render_glint(&mut pass, &self.local_camera_bind_group, batches);
        }

        // `renderEntities` draws the labels with the entities, before the
        // particles and the translucent terrain.
        // With the motion blur they come after it instead: the blur moves a pixel
        // by the depth under it, and a tag seen through a wall has the wall's.
        if !motion {
            self.nametags
                .draw(&mut pass, &self.overlay_pipeline, &self.ui_bind_group);
        }

        // Dropped items share the terrain pipeline and its texture array.
        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.local_terrain_bind_group, &[]);
        self.world_items.draw(&mut pass);
        self.world_glint
            .draw_world(&mut pass, &self.local_terrain_bind_group);
        self.stamp(&mut pass, 3);

        // `renderWorldPass` draws the selection box right after the entities,
        // before the particles, the rain and the translucent layer: they hide
        // it and water and glass tint it. With the eyes in water it comes
        // after the translucent layer instead.
        let eyes_in_water = self.screen_overlays.underwater.is_some();
        if !eyes_in_water {
            self.outline.draw(&mut pass, &self.camera_bind_group);
        }

        // The clouds hide behind terrain and hide the water behind them, as in
        // `renderWorldPass`: before the translucent layer from below.
        self.clouds.draw_below(&mut pass);

        // `renderParticles` follows the entities; `renderRainSnow` comes after
        // both and before the translucent terrain.
        self.particles
            .draw(&mut pass, &self.local_terrain_bind_group);
        draw_stats.translucent += self.weather.draw(&mut pass, &self.local_camera_bind_group);

        // Transparent terrain: water/glass, farthest chunk first, with
        // depth testing but no depth writes so blended faces do not occlude
        // each other by submission order.
        pass.set_pipeline(&self.terrain_transparent_pipeline);
        pass.set_bind_group(0, &self.chunk_bind_group, &[]);
        self.quad_indices.bind(&mut pass);
        for draw in &self.visible_transparent {
            let Some(slot) = self.mesh_slots.get(draw.slot as usize).and_then(Option::as_ref) else {
                continue;
            };
            let Some(transparent) = &slot.mesh.transparent else {
                continue;
            };
            let mut bound = false;
            for range in draw.runs.as_slice() {
                if !bound {
                    pass.set_vertex_buffer(0, transparent.vertex_buffer.slice(..));
                    bound = true;
                }
                pass.draw_indexed(range.clone(), 0, 0..1);
                draw_stats.translucent += 1;
            }
        }
        // Drawn after the chunks' blended faces: a handful of quads, so no
        // sorting against them.
        self.moving_blocks.draw_transparent(&mut pass);

        // The world border follows the weather and precedes the high clouds.
        self.border.draw(&mut pass);

        // From above 128 the clouds come after the translucent layer.
        self.clouds.draw_above(&mut pass);

        if let Some(cracks) = self.crack_mesh.as_ref() {
            pass.set_pipeline(&self.crack_pipeline);
            pass.set_bind_group(0, &self.terrain_bind_group, &[]);
            draw_gpu_pass(&mut pass, cracks);
        }

        // Under water the box comes after the translucent layer instead.
        if eyes_in_water {
            self.outline.draw(&mut pass, &self.camera_bind_group);
        }
        self.stamp(&mut pass, 4);

        if motion {
            // The hand goes into the picture that is blurred: it trails when it
            // moves. The overlays and the HUD come after, sharp.
            self.draw_hand(&mut pass);
            // The world is done: blur it, lay it over the screen, and draw the
            // overlays and the HUD sharp on top. The depth buffer is kept.
            drop(pass);
            self.motion_blur.run(encoder);
            pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("motion blur composite pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.motion_blur.blit(&mut pass);
            // Sharp, and still tested against the depth buffer of the world.
            self.nametags
                .draw(&mut pass, &self.overlay_pipeline, &self.ui_bind_group);
        }

        if !motion {
            self.draw_hand(&mut pass);
        }

        // `renderOverlays` follows the hand; the GUI's vignette, pumpkin and
        // portal come before the hotbar.
        self.screen.draw(&mut pass, &self.terrain_bind_group);

        if blur {
            // The world is done: smooth it, lay it over the screen, and draw
            // the HUD and the menu sharp on top.
            drop(pass);
            self.scene_blur.blur(encoder);
            pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blurred pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
            self.scene_blur.blit(&mut pass);
        }

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
                    UiBatchKind::BrandFont => {
                        // `UiPass` only emits this kind with a brand atlas,
                        // which `set_brand_font` uploads together.
                        let Some(brand) = &self.brand_bind_group else {
                            continue;
                        };
                        pass.set_pipeline(&self.brand_pipeline);
                        pass.set_bind_group(0, brand, &[]);
                    }
                    UiBatchKind::Icon => {
                        pass.set_pipeline(&self.gui_overlay_pipeline);
                        pass.set_bind_group(0, &self.icon_gui_bind_group, &[]);
                    }
                    UiBatchKind::IconGlint => {
                        if !self.glint.bind(&mut pass) {
                            continue;
                        }
                    }
                    UiBatchKind::Gui | UiBatchKind::GuiInverted => {
                        if let Some(gui_bg) = &self.gui_bind_group {
                            pass.set_pipeline(if batch.kind == UiBatchKind::Gui {
                                &self.gui_overlay_pipeline
                            } else {
                                &self.gui_inverted_pipeline
                            });
                            pass.set_bind_group(0, gui_bg, &[]);
                        } else {
                            // No GUI atlas uploaded — skip this batch.
                            continue;
                        }
                    }
                }
                pass.draw(first..end, 0..1);
                draw_stats.ui += 1;
            }
        }
        if !overlay.is_empty() {
            draw_stats.ui += 1;
        }
        self.last_draw_calls.set(draw_stats);
        self.stamp(&mut pass, 5);
    }

    /// Draws the SOLID (`solid`) or the CUTOUT quads of every visible opaque
    /// chunk, nearest first. The quad index buffer and the pipeline are bound
    /// by the caller. Pre-resolved ChunkRuns avoid per-frame section bit loops
    /// and eliminate hash map lookups.
    fn draw_visible_opaque(&self, pass: &mut wgpu::RenderPass<'_>, solid: bool) -> u32 {
        let mut draws = 0_u32;
        let list = if solid {
            &self.visible_solid
        } else {
            &self.visible_cutout
        };
        for draw in list {
            let Some(slot) = self.mesh_slots.get(draw.slot as usize).and_then(Option::as_ref) else {
                continue;
            };
            let Some(opaque) = &slot.mesh.opaque else {
                continue;
            };
            let mut bound = false;
            for range in draw.runs.as_slice() {
                if !bound {
                    pass.set_vertex_buffer(0, opaque.vertex_buffer.slice(..));
                    bound = true;
                }
                pass.draw_indexed(range.clone(), 0, 0..1);
                draws += 1;
            }
        }
        draws
    }

    fn reconfigure_surface(&mut self) -> Result<(), RenderError> {
        self.frame_pacing.reconfigure_count += 1;
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

/// The present mode of the "Use VSync" option: `Auto*` picks the best mode
/// the platform supports for the wish (Fifo for vsync, Immediate or Mailbox
/// without).
fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
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
        // Timestamps are optional: without them F3 shows "n/a".
        required_features: if adapter.features().contains(gpu_timer::REQUIRED_FEATURES) {
            gpu_timer::REQUIRED_FEATURES
        } else {
            wgpu::Features::empty()
        },
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
        // Sampled by the motion blur, which works out the motion from the depth.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
        format: wgpu::TextureFormat::Rgba8Unorm,
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
    cull_mode: Option<wgpu::Face>,
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
            cull_mode: Some(wgpu::Face::Back),
        }
    }

    /// The opaque pass for quads that never need the alpha test: the same
    /// state, a fragment shader without `discard`, so the GPU keeps early
    /// depth rejection.
    fn solid() -> Self {
        Self {
            label: "terrain solid pipeline",
            fragment_entry: "fs_solid",
            ..Self::opaque()
        }
    }

    /// Item icons: the opaque pass without back-face culling, because the
    /// GUI chains mirror space and the reference draws icons uncullled.
    fn icon() -> Self {
        Self {
            label: "icon pipeline",
            fragment_entry: "fs_icon",
            cull_mode: None,
            ..Self::opaque()
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
            cull_mode: Some(wgpu::Face::Back),
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
            cull_mode: Some(wgpu::Face::Back),
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
            cull_mode: state.cull_mode,
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
pub(crate) fn create_gpu_mesh_pass(
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
pub(crate) fn draw_gpu_pass(pass: &mut wgpu::RenderPass<'_>, mesh: &GpuMeshPass) {
    draw_gpu_pass_buffers(
        pass,
        &mesh.vertex_buffer,
        &mesh.index_buffer,
        mesh.index_count,
    );
}

/// Binds and draws the first `index_count` indices of two mesh buffers.
pub(crate) fn draw_gpu_pass_buffers(
    pass: &mut wgpu::RenderPass<'_>,
    vertices: &wgpu::Buffer,
    indices: &wgpu::Buffer,
    index_count: u32,
) {
    pass.set_vertex_buffer(0, vertices.slice(..));
    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..index_count, 0, 0..1);
}

/// The chunk centre used for transparent sorting: the middle of the 16×16
/// column at the cloud/terrain reference height of y = 128.
#[cfg_attr(not(test), allow(dead_code))]
#[inline]
fn chunk_centre(pos: ChunkPos) -> DVec3 {
    DVec3::new(
        f64::from(pos.x) * 16.0 + 8.0,
        128.0,
        f64::from(pos.z) * 16.0 + 8.0,
    )
}

/// True when the column is within `reach` chunks of the eye (a square, like
/// the reference's `renderDistanceChunks`) and its bounding box touches the
/// frustum. `frustum` and the box are relative to `origin`.
#[allow(dead_code)]
fn chunk_is_visible(
    pos: ChunkPos,
    eye: ChunkPos,
    reach: i32,
    frustum: &Frustum,
    origin: DVec3,
) -> bool {
    if (pos.x - eye.x).abs() > reach || (pos.z - eye.z).abs() > reach {
        return false;
    }
    let min = DVec3::new(f64::from(pos.x) * 16.0, 0.0, f64::from(pos.z) * 16.0) - origin;
    let min = min.as_vec3();
    frustum.intersects_aabb(min, min + Vec3::new(16.0, 256.0, 16.0))
}

/// The sections of column `pos` whose 16³ box touches the frustum (bit `s` =
/// section `s`). Boxes are relative to the render `origin`.
///
/// Uses $O(1)$ plane-column interval projection without evaluating each section box individually.
fn sections_in_frustum(pos: ChunkPos, frustum: &Frustum, origin: DVec3) -> u16 {
    let base = DVec3::new(f64::from(pos.x) * 16.0, 0.0, f64::from(pos.z) * 16.0) - origin;
    frustum.sections_in_column(base.x as f32, base.y as f32, base.z as f32)
}

/// Squared camera-to-chunk-centre distance; avoids a square root per chunk.
fn squared_distance_to_chunk(pos: ChunkPos, eye: DVec3) -> f32 {
    let dx = f64::from(pos.x) * 16.0 + 8.0 - eye.x;
    let dy = 128.0 - eye.y;
    let dz = f64::from(pos.z) * 16.0 + 8.0 - eye.z;
    (dx * dx + dy * dy + dz * dz) as f32
}

/// Orders opaque chunks nearest first, so what stands in front is drawn
/// before what hides behind it and fewer fragments are shaded for nothing
/// (F2-2b); `ChunkPos` is the deterministic tie-break.
#[cfg(test)]
#[allow(dead_code)]
fn sort_nearest_first(chunks: &mut [ChunkDraw]) {
    chunks.sort_unstable_by(|a, b| {
        a.distance_sq
            .total_cmp(&b.distance_sq)
            .then_with(|| a.pos.cmp(&b.pos))
    });
}

/// Orders transparent chunks farthest first, with `ChunkPos` as the
/// deterministic tie-break (ADR-0008 §3).
#[cfg(test)]
fn sort_transparent_farthest_first(chunks: &mut [ChunkDraw]) {
    chunks.sort_unstable_by(|a, b| {
        b.distance_sq
            .total_cmp(&a.distance_sq)
            .then_with(|| a.pos.cmp(&b.pos))
    });
}

/// Binds the camera uniform, one block texture-array view, the block sampler
/// and the second array of large textures with its layer map.
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

/// Builds a UI overlay pipeline over [`OverlayVertex`] quads with `blend`.
pub(crate) fn create_overlay_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    create_overlay_pipeline_with_entry(device, shader, bind_group_layout, format, blend, "fs_main")
}

/// [`create_overlay_pipeline`] with a named fragment entry point of `shader`.
fn create_overlay_pipeline_with_entry(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    blend: wgpu::BlendState,
    fragment_entry: &str,
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
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
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
    fn the_vsync_option_selects_the_present_mode() {
        assert_eq!(present_mode(true), wgpu::PresentMode::AutoVsync);
        assert_eq!(present_mode(false), wgpu::PresentMode::AutoNoVsync);
    }

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
            fog_density: 0.1,
            _pad: 0.0,
            origin: [0.0; 3],
            _pad_origin: 0.0,
            lightmap: lightmap::unlit(),
        };
        let bytes = bytemuck::bytes_of(&uniform);
        assert_eq!(bytes.len(), 128 + 256 * 16);
        assert_eq!(bytes.len() % 16, 0);
        // These offsets must match the WGSL `Camera` struct in terrain.wgsl.
        assert_eq!(std::mem::offset_of!(CameraUniform, camera_pos), 64);
        assert_eq!(std::mem::offset_of!(CameraUniform, sun_brightness), 76);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_color), 80);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_start), 92);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_end), 96);
        assert_eq!(std::mem::offset_of!(CameraUniform, gamma), 100);
        assert_eq!(std::mem::offset_of!(CameraUniform, fog_density), 104);
        assert_eq!(std::mem::offset_of!(CameraUniform, origin), 112);
        assert_eq!(std::mem::offset_of!(CameraUniform, lightmap), 128);
    }

    #[test]
    fn transparent_sort_is_farthest_first() {
        let camera = DVec3::ZERO;
        let mut chunks = vec![
            ChunkDraw {
                pos: ChunkPos::new(0, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(0, 0), camera),
                sections: 1,
                has_solid: true,
                has_cutout: false,
            },
            ChunkDraw {
                pos: ChunkPos::new(2, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(2, 0), camera),
                sections: 1,
                has_solid: true,
                has_cutout: false,
            },
            ChunkDraw {
                pos: ChunkPos::new(1, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(1, 0), camera),
                sections: 1,
                has_solid: true,
                has_cutout: false,
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
        .map(|pos| ChunkDraw {
            pos,
            distance_sq: squared_distance_to_chunk(pos, camera),
            sections: 1,
            has_solid: true,
            has_cutout: false,
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
        let mut chunks: Vec<ChunkDraw> = Vec::new();
        sort_transparent_farthest_first(&mut chunks);
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_centre_is_the_column_middle() {
        assert_eq!(
            chunk_centre(ChunkPos::new(-1, 2)),
            DVec3::new(-8.0, 128.0, 40.0)
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
        let module = naga::front::wgsl::parse_str(TERRAIN_WGSL).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in [
            "fs_main",
            "fs_solid",
            "fs_icon",
            "fs_transparent",
            "fs_crack",
        ] {
            assert!(
                module.entry_points.iter().any(|point| point.name == entry),
                "missing {entry}"
            );
        }
    }
}
