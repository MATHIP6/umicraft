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
use crate::gpu_timer::{self, GpuTimer, GpuTimings};
use crate::hires_blocks::{self, HiresBlocks};
use crate::items::{DynamicMesh, GlintPass, IconAtlas, IconProvider, WorldGlint};
use crate::mipmap;
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

/// Frames the presentation queue may hold (F2-5e). The wgpu default is 2, which
/// lets the CPU run a frame ahead of the display and adds up to a frame of
/// input latency with vsync; 1 keeps the pipeline full for a game whose frame
/// fits the refresh. Not measured on a display here: revert to 2 if a
/// GPU-bound scene loses frames.
const FRAME_LATENCY: u32 = 1;

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
#[derive(Debug, Clone, Copy, PartialEq)]
struct ChunkDraw {
    pos: ChunkPos,
    distance_sq: f32,
    /// The sections to draw (bit `s` = section `s`): in the frustum and not
    /// hidden by the cave graph.
    sections: u16,
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
    /// Reused per-frame queue of transparent chunks, farthest first. Its
    /// capacity is bounded by the number of uploaded meshes and is retained
    /// across frames, so sorting allocates nothing in steady state.
    transparent_order: Vec<ChunkDraw>,
    /// Reused per-frame list of the chunks with opaque geometry that are
    /// inside the render distance and the view frustum.
    visible_opaque: Vec<ChunkDraw>,
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
    chunks: ChunkMap<GpuMesh>,
    camera_bind_group: wgpu::BindGroup,
    entity_renderer: EntityRenderer,
    outline: OutlineRenderer,
    /// The blur of the game behind a menu, and whether a menu asks for it.
    scene_blur: SceneBlur,
    background_blur: bool,
    entity_batches: BatchBuffer,
    /// The glowing layers of the entities, drawn after them.
    additive_batches: BatchBuffer,
    /// The enchantment glint of worn armour, drawn over it at equal depth.
    glint_batches: BatchBuffer,
    /// Invisible entities a spectator sees, drawn translucent.
    ghost_batches: BatchBuffer,
    /// The shadows under the entities.
    shadow_batches: BatchBuffer,
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
        config.desired_maximum_frame_latency = FRAME_LATENCY;
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
            transparent_order: Vec::with_capacity(64),
            visible_opaque: Vec::with_capacity(256),
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
            camera_bind_group,
            entity_renderer,
            outline,
            scene_blur,
            background_blur: false,
            entity_batches: BatchBuffer::default(),
            additive_batches: BatchBuffer::default(),
            glint_batches: BatchBuffer::default(),
            ghost_batches: BatchBuffer::default(),
            shadow_batches: BatchBuffer::default(),
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
    pub fn render_portrait(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
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
        let (entities, camera) = (&self.entity_renderer, &self.icon_entity_bind_group);
        self.icon_atlas
            .render_portrait(&self.device, &self.queue, |pass| {
                entities.render_icon(
                    pass,
                    camera,
                    buffers.iter().map(|(b, n, t)| (b.slice(..), *n, *t)),
                );
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
        if self.surface.is_some() {
            self.render_window(overlay, &sky_state)
        } else {
            self.render_offscreen(overlay, &sky_state)
        }
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
        let frustum = Frustum::from_view_projection(camera.view_projection_from(origin));
        let eye = camera.eye();
        let eye_chunk = ChunkPos::new((eye.x / 16.0).floor() as i32, (eye.z / 16.0).floor() as i32);
        let reach = i32::from(self.environment.view_distance);
        self.visible_opaque.clear();
        self.transparent_order.clear();
        let chunks = &self.chunks;
        if self.cave_culling {
            self.cave_graph
                .update((eye.x, eye.y, eye.z), reach, self.chunks_version, |pos| {
                    chunks.get(&pos).map(|mesh| &mesh.connectivity)
                });
        }
        let cave = self.cave_culling.then_some(&self.cave_graph);
        for (pos, mesh) in chunks {
            if !chunk_is_visible(*pos, eye_chunk, reach, &frustum, origin) {
                continue;
            }
            let mut sections = sections_in_frustum(*pos, &frustum, origin);
            if let Some(cave) = cave {
                sections &= cave.mask(*pos);
            }
            if sections == 0 {
                continue;
            }
            let draw = ChunkDraw {
                pos: *pos,
                distance_sq: squared_distance_to_chunk(*pos, eye),
                sections,
            };
            if mesh.opaque.is_some() {
                self.visible_opaque.push(draw);
            }
            if mesh.transparent.is_some() {
                self.transparent_order.push(draw);
            }
        }
        sort_nearest_first(&mut self.visible_opaque);
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
    }

    /// Uploads the enchantment glint texture (`enchanted_item_glint.png`).
    ///
    /// `rgba` must be `width * height * 4` bytes; a malformed upload is
    /// ignored. Until a texture is set, glinting icons and items draw without the
    /// glint.
    pub fn set_glint_texture(&mut self, width: u32, height: u32, rgba: &[u8]) {
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
        let chains = mipmap::generate_chains(pixels, layer_bytes, (width, height), levels);
        for level in 1..=levels {
            let joined: Vec<u8> = chains
                .iter()
                .flat_map(|chain| chain[level as usize - 1].iter().copied())
                .collect();
            let size = ((width >> level).max(1), (height >> level).max(1), layers);
            self.write_block_level(texture, level, size, &joined);
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
            self.chunks.remove(&pos);
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
            self.chunks.remove(&pos);
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
        if let Entry::Occupied(mut entry) = self.chunks.entry(pos) {
            *entry.get_mut() = mesh;
        } else {
            self.chunks.insert(pos, mesh);
        }
    }

    /// Removes the GPU mesh for one chunk column.
    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks_version += 1;
        self.chunks.remove(&pos);
    }

    /// Removes every uploaded chunk mesh (e.g. after a dimension change).
    pub fn clear_meshes(&mut self) {
        self.chunks_version += 1;
        self.chunks.clear();
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
        let (Some(config), Some(surface)) = (self.surface_config.as_mut(), self.surface.as_ref())
        else {
            return;
        };
        if config.present_mode != mode {
            config.present_mode = mode;
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

    fn write_camera(&self, camera: &Camera, sky: &SkyState, flicker: f32) {
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
            lightmap: lightmap::colors(&LightmapParams::of(
                &self.environment,
                sky.sun_brightness,
                flicker,
            )),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        // Same matrix and eye: the vertices of this one are already relative.
        let local = CameraUniform {
            origin: [0.0; 3],
            ..uniform
        };
        self.queue
            .write_buffer(&self.local_camera_buffer, 0, bytemuck::bytes_of(&local));

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
        self.queue.write_buffer(
            &self.hand_camera_buffer,
            0,
            bytemuck::bytes_of(&hand_uniform),
        );
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
                self.begin_gpu_timing();
                self.draw(&view, overlay, &mut encoder, sky_state);
                self.submit_timed(encoder);
                self.capture_frame(&frame.texture);
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
        let world_view = if blur {
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
        self.sky.draw(&mut pass);
        self.panorama.draw(&mut pass);
        self.stamp(&mut pass, 1);

        // Opaque terrain, nearest chunk first: the SOLID quads (no alpha
        // test, early depth rejection) and then the CUTOUT ones, which
        // discard texels. Both write depth and do not depend on the order.
        self.quad_indices.bind(&mut pass);
        pass.set_bind_group(0, &self.chunk_bind_group, &[]);
        pass.set_pipeline(&self.terrain_solid_pipeline);
        self.draw_visible_opaque(&mut pass, true);
        pass.set_pipeline(&self.terrain_pipeline);
        self.draw_visible_opaque(&mut pass, false);
        self.moving_blocks.draw_opaque(&mut pass);
        self.stamp(&mut pass, 2);

        // The shadows lie on the ground, drawn before the entities.
        if !self.shadow_batches.is_empty() {
            let batches = self.shadow_batches.iter();
            self.entity_renderer
                .render_shadows(&mut pass, &self.local_camera_bind_group, batches);
        }

        // Entities: depth write on, cutout discard, rendered between opaque and transparent passes.
        if !self.entity_batches.is_empty() {
            let batches = self.entity_batches.iter();
            self.entity_renderer
                .render(&mut pass, &self.local_camera_bind_group, batches);
        }

        // The invisible entities a spectator sees, over the opaque ones.
        if !self.ghost_batches.is_empty() {
            let batches = self.ghost_batches.iter();
            self.entity_renderer
                .render_ghosts(&mut pass, &self.local_camera_bind_group, batches);
        }

        // The glow of the entities is added over them, without writing depth.
        if !self.additive_batches.is_empty() {
            let batches = self.additive_batches.iter();
            self.entity_renderer
                .render_additive(&mut pass, &self.local_camera_bind_group, batches);
        }

        if !self.glint_batches.is_empty() {
            let batches = self.glint_batches.iter();
            self.entity_renderer
                .render_glint(&mut pass, &self.local_camera_bind_group, batches);
        }

        // `renderEntities` draws the labels with the entities, before the
        // particles and the translucent terrain.
        self.nametags
            .draw(&mut pass, &self.overlay_pipeline, &self.ui_bind_group);

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
        self.weather.draw(&mut pass, &self.local_camera_bind_group);

        // Transparent terrain: water/glass, farthest chunk first, with
        // depth testing but no depth writes so blended faces do not occlude
        // each other by submission order.
        pass.set_pipeline(&self.terrain_transparent_pipeline);
        pass.set_bind_group(0, &self.chunk_bind_group, &[]);
        self.quad_indices.bind(&mut pass);
        for draw in &self.transparent_order {
            let Some(transparent) = self
                .chunks
                .get(&draw.pos)
                .and_then(|mesh| mesh.transparent.as_ref())
            else {
                continue;
            };
            transparent.visible_runs(draw.sections, Group::Solid, |indices| {
                pass.set_vertex_buffer(0, transparent.vertex_buffer.slice(..));
                pass.draw_indexed(indices, 0, 0..1);
            });
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

        // The hand: camera-space vertices under a squeezed depth range.
        if !self.hand.is_empty() {
            self.entity_renderer
                .render(&mut pass, &self.hand_camera_bind_group, self.hand.iter());
        }

        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.hand_item_bind_group, &[]);
        self.hand_items.draw(&mut pass);
        self.world_glint
            .draw_hand(&mut pass, &self.hand_item_bind_group);

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
            }
        }
        self.stamp(&mut pass, 5);
    }

    /// Draws the SOLID (`solid`) or the CUTOUT quads of every visible opaque
    /// chunk, nearest first. The quad index buffer and the pipeline are bound
    /// by the caller. A chunk sorted with an outdated atlas has no valid
    /// split: it is drawn whole, with the alpha test.
    fn draw_visible_opaque(&self, pass: &mut wgpu::RenderPass<'_>, solid: bool) {
        for draw in &self.visible_opaque {
            let Some(opaque) = self
                .chunks
                .get(&draw.pos)
                .and_then(|mesh| mesh.opaque.as_ref())
            else {
                continue;
            };
            let mut run = |indices: std::ops::Range<u32>| {
                pass.set_vertex_buffer(0, opaque.vertex_buffer.slice(..));
                pass.draw_indexed(indices, 0, 0..1);
            };
            match (opaque.epoch == self.cutout_epoch, solid) {
                (true, true) => opaque.visible_runs(draw.sections, Group::Solid, &mut run),
                (true, false) => opaque.visible_runs(draw.sections, Group::Cutout, &mut run),
                (false, true) => {}
                (false, false) => {
                    opaque.visible_runs(draw.sections, Group::Solid, &mut run);
                    opaque.visible_runs(draw.sections, Group::Cutout, &mut run);
                }
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
fn sections_in_frustum(pos: ChunkPos, frustum: &Frustum, origin: DVec3) -> u16 {
    let base = DVec3::new(f64::from(pos.x) * 16.0, 0.0, f64::from(pos.z) * 16.0) - origin;
    (0..SECTIONS).fold(0_u16, |mask, section| {
        let min = (base + DVec3::new(0.0, section as f64 * 16.0, 0.0)).as_vec3();
        mask | u16::from(frustum.intersects_aabb(min, min + Vec3::splat(16.0))) << section
    })
}

/// Squared camera-to-chunk-centre distance; avoids a square root per chunk.
fn squared_distance_to_chunk(pos: ChunkPos, eye: DVec3) -> f32 {
    chunk_centre(pos).distance_squared(eye) as f32
}

/// Orders opaque chunks nearest first, so what stands in front is drawn
/// before what hides behind it and fewer fragments are shaded for nothing
/// (F2-2b); `ChunkPos` is the deterministic tie-break.
fn sort_nearest_first(chunks: &mut [ChunkDraw]) {
    chunks.sort_unstable_by(|a, b| {
        a.distance_sq
            .total_cmp(&b.distance_sq)
            .then_with(|| a.pos.cmp(&b.pos))
    });
}

/// Orders transparent chunks farthest first, with `ChunkPos` as the
/// deterministic tie-break (ADR-0008 §3).
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
            },
            ChunkDraw {
                pos: ChunkPos::new(2, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(2, 0), camera),
                sections: 1,
            },
            ChunkDraw {
                pos: ChunkPos::new(1, 0),
                distance_sq: squared_distance_to_chunk(ChunkPos::new(1, 0), camera),
                sections: 1,
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
