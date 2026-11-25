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

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::Vertex;
use umicraft_ui::FontAtlas;
use umicraft_world::ChunkMap;
use winit::window::Window;

use crate::adapter::AdapterSummary;
use crate::batch_buffer::BatchBuffer;
use crate::border::BorderRenderer;
use crate::cave_graph::CaveGraph;
use crate::chunk_pass::QuadIndices;
use crate::clouds::CloudRenderer;
use crate::dynamic_blocks::MovingBlocksPass;
use crate::entity_mask::EntityMaskRenderer;
use crate::entity_pass::EntityRenderer;
use crate::environment::RenderEnvironment;
use crate::error::RenderError;
use crate::frame_pacing::FramePacingTracker;
use crate::gpu_timer::{DrawCallStats, GpuTimer};
use crate::hires_blocks::HiresBlocks;
use crate::items::{DynamicMesh, GlintPass, IconAtlas, WorldGlint};
use crate::lightmap::{self, TorchFlicker};
use crate::mod_world::ModWorldRenderer;
use crate::motion_blur::MotionBlur;
use crate::nametag::NametagRenderer;
use crate::outline::LineRenderer;
use crate::overlays::{ScreenOverlays, ScreenRenderer};
use crate::panorama::PanoramaRenderer;
use crate::particles::{ParticleRenderer, ParticleSprite};
use crate::post_chain::PostChain;
use crate::scene_blur::SceneBlur;
use crate::sky::SkyRenderer;
use crate::ui_pass::UiPass;
use crate::weather::WeatherRenderer;
use surface::OffscreenTarget;
use terrain::{MeshSlot, OpaqueCandidate, PreparedChunkDraw, TransparentCandidate};

mod draw;
mod entities;
mod frame;
mod icons;
mod init;
mod pipelines;
mod readback;
mod resources;
mod scene;
mod stats;
mod surface;
mod terrain;
#[cfg(test)]
mod tests;
mod textures;

pub(crate) use pipelines::{
    create_gpu_mesh_pass, create_overlay_pipeline, draw_gpu_pass, draw_gpu_pass_buffers,
};

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
    outline: LineRenderer,
    /// The coloured lines of the debug chords (hitboxes, chunk borders).
    debug_lines: LineRenderer,
    /// The lines and shapes the mods draw in the world (API 1.4).
    mod_world: ModWorldRenderer,
    /// The entities the mods marked for the mask of their post passes (API 1.4).
    entity_mask: EntityMaskRenderer,
    /// The blur of the game behind a menu, and whether a menu asks for it.
    scene_blur: SceneBlur,
    /// The motion blur of the world (the Motion Blur mod).
    motion_blur: MotionBlur,
    background_blur: bool,
    /// The post-processing passes of the mods, run on the scene texture.
    post_chain: PostChain,
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
