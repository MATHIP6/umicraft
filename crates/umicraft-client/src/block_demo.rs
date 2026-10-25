// SPDX-License-Identifier: MIT OR Apache-2.0

//! The server-less M5/M6 acceptance run (`--block-demo`).
//!
//! The demo loads the user's official 1.8.8 blockstates, models and textures,
//! builds a synthetic `ChunkColumn` with the acceptance blocks, meshes it with
//! [`mesh_chunk_with_models`] and prints the counts frozen in ADR-0007. With
//! `--screenshot PATH` it also uploads the real texture array, renders one
//! offscreen frame and writes the PNG; without it the report needs neither a
//! server nor a GPU.
//!
//! The scene is original content: a stone platform and four stepped rows of
//! blocks. It contains stone, dirt, grass, planks, cobblestone, sandstone,
//! bricks, glass, leaves, water, every 1.8 ore, obsidian, glowstone, all 16 wool
//! colours, pumpkin, melon, netherrack, quartz, and a shape row with slabs,
//! stairs, fence, pane, torch, carpet, snow layer, cactus, ladder, rail and
//! vine. The M6 wing adds a flat sky-lit area at y = 65 with an AO corner, a
//! lit cave, a water pool, glass window and leaf canopy. The real textures and
//! geometry come from the installation at runtime; no Mojang asset is copied
//! into the repository.

use std::sync::Arc;

use glam::Vec3;
use umicraft_mesh::{mesh_chunk_with_models, ChunkSnapshot, Mesh, ModelMeshContext, VertexLight};
use umicraft_render::{Camera, HudLayout, Overlay, RenderEnvironment, Renderer};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

use crate::args::{Args, DemoView};
use crate::error::ClientError;
use crate::model_resources::{ModelResources, ModelStats};
use crate::screenshot::write_png;
use crate::terrain_textures::TerrainTextures;

/// The single meshed column of the demo.
const DEMO_CHUNK: ChunkPos = ChunkPos::new(0, 0);
/// Plains biome id (1.8 `BiomeGenBase.plains`), used by the whole demo.
const PLAINS: u8 = 1;
/// The full stone platform sits on this layer.
const PLATFORM_Y: usize = 64;
/// The M6 wing ground level (one block above the platform).
const M6_GROUND_Y: usize = 65;
/// The top of the wing's two-block walls.
const M6_WALL_TOP_Y: usize = 67;
/// The camera height, chosen to see all four stepped rows.
const CAMERA_Y: f32 = 72.0;
/// The camera distance behind the platform.
const CAMERA_Z: f32 = 24.0;
/// The downward camera pitch of the acceptance shot.
const CAMERA_PITCH: f32 = -0.35;

/// The cave interior spans these local coordinates (a 3×3×3 air room).
const CAVE_MIN: [usize; 3] = [5, 61, 5];
/// Exclusive upper bound of the cave interior.
const CAVE_MAX: [usize; 3] = [8, 64, 8];
/// The glowstone cell in the cave's west wall.
const CAVE_GLOWSTONE: [usize; 3] = [4, 62, 6];

/// Camera position and orientation for one acceptance view.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DemoCamera {
    /// Eye position in world coordinates.
    position: Vec3,
    /// Horizontal rotation in radians; `0` looks toward −Z.
    yaw: f32,
    /// Vertical rotation in radians; negative looks down.
    pitch: f32,
}

/// The overview camera, unchanged from the M5 acceptance shot.
const OVERVIEW_CAMERA: DemoCamera = DemoCamera {
    position: Vec3::new(8.0, CAMERA_Y, CAMERA_Z),
    yaw: 0.0,
    pitch: CAMERA_PITCH,
};
/// The day/night camera: north of the platform, framing the AO corner,
/// the glass window over the pool and the silhouette pillar.
const GROUND_CAMERA: DemoCamera = DemoCamera {
    position: Vec3::new(3.0, 69.0, -6.0),
    yaw: std::f32::consts::PI,
    pitch: -0.39,
};
/// The cave camera: inside the room, facing the glowstone in the west wall.
const CAVE_CAMERA: DemoCamera = DemoCamera {
    position: Vec3::new(6.5, 62.6, 6.0),
    yaw: -std::f32::consts::FRAC_PI_2,
    pitch: -0.05,
};
/// The transparency camera: looking at the pool, the glass window and the
/// leaf canopy in front of the wing's stone wall.
const TRANSPARENCY_CAMERA: DemoCamera = DemoCamera {
    position: Vec3::new(4.5, 68.0, -4.5),
    yaw: std::f32::consts::PI,
    pitch: -0.24,
};

/// One acceptance block and the label used by the report.
#[derive(Debug, Clone, Copy)]
struct DemoBlock {
    label: &'static str,
    state: BlockState,
}

/// A scene block from an id and metadata.
const fn block(label: &'static str, id: u16, metadata: u8) -> DemoBlock {
    DemoBlock {
        label,
        state: BlockState::new(id, metadata),
    }
}

/// The cube row: common blocks and the every-ore set.
const CUBE_ROW: &[DemoBlock] = &[
    block("stone", 1, 0),
    block("dirt", 3, 0),
    block("grass", 2, 0),
    block("oak planks", 5, 0),
    block("cobblestone", 4, 0),
    block("sandstone", 24, 0),
    block("bricks", 45, 0),
    block("glass", 20, 0),
    block("oak leaves", 18, 0),
    block("water", 9, 0),
    block("obsidian", 49, 0),
    block("glowstone", 89, 0),
    block("pumpkin", 86, 0),
];

/// The plant and ore row, including two wool colours.
const ORE_ROW: &[DemoBlock] = &[
    block("melon", 103, 0),
    block("netherrack", 87, 0),
    block("quartz block", 155, 0),
    block("coal ore", 16, 0),
    block("iron ore", 15, 0),
    block("gold ore", 14, 0),
    block("diamond ore", 56, 0),
    block("redstone ore", 73, 0),
    block("lapis ore", 21, 0),
    block("emerald ore", 129, 0),
    block("quartz ore", 153, 0),
    block("white wool", 35, 0),
    block("orange wool", 35, 1),
];

/// The middle wool colours (metadata 2-14).
const WOOL_ROW: &[DemoBlock] = &[
    block("magenta wool", 35, 2),
    block("light blue wool", 35, 3),
    block("yellow wool", 35, 4),
    block("lime wool", 35, 5),
    block("pink wool", 35, 6),
    block("gray wool", 35, 7),
    block("light gray wool", 35, 8),
    block("cyan wool", 35, 9),
    block("purple wool", 35, 10),
    block("blue wool", 35, 11),
    block("brown wool", 35, 12),
    block("green wool", 35, 13),
    block("red wool", 35, 14),
];

/// The black wool and the non-cube shape row.
const SHAPE_ROW: &[DemoBlock] = &[
    block("black wool", 35, 15),
    block("stone slab (bottom)", 44, 0),
    block("stone slab (top)", 44, 8),
    block("oak stairs", 53, 0),
    block("oak fence", 85, 0),
    block("glass pane", 102, 0),
    block("torch", 50, 0),
    block("red carpet", 171, 14),
    block("snow layer", 78, 3),
    block("cactus", 81, 0),
    block("oak ladder", 65, 3),
    block("rail", 66, 0),
    block("vine", 106, 15),
];

/// The four rows, nearest to farthest.
const ROWS: [&[DemoBlock]; 4] = [CUBE_ROW, ORE_ROW, WOOL_ROW, SHAPE_ROW];

/// The synthetic scene and the labels of its acceptance blocks.
struct DemoScene {
    center: Arc<ChunkColumn>,
    neighbours: Vec<Arc<ChunkColumn>>,
    labels: Vec<&'static str>,
}

impl DemoScene {
    /// Builds the platform, the stepped rows and the empty 3×3 ring.
    fn build() -> Self {
        let mut column = ChunkColumn::new(DEMO_CHUNK);
        // Plains everywhere: the demo grass, leaves and water must sample the
        // plains palette so the screenshot shows the 1.8 green (ADR-0009).
        column.fill_biomes(PLAINS);
        for z in 0..16 {
            for x in 0..16 {
                column.set_block(x, PLATFORM_Y, z, BlockState::new(1, 0));
            }
        }

        let mut labels = Vec::with_capacity(ROWS.iter().map(|row| row.len()).sum());
        for (row_index, row) in ROWS.iter().enumerate() {
            let y = PLATFORM_Y + 1 + row_index;
            let z = 12 - row_index * 3;
            for (index, demo) in row.iter().enumerate() {
                let x = 1 + index;
                // Higher rows stand on stone supports so nothing floats.
                for support_y in PLATFORM_Y + 1..y {
                    column.set_block(x, support_y, z, BlockState::new(1, 0));
                }
                column.set_block(x, y, z, demo.state);
                labels.push(demo.label);
            }
        }

        // M6 wing: ground with an AO corner, a glass window, a water pool, a
        // leaf canopy, a silhouette pillar and a lit cave below the platform.
        Self::build_m6_wing(&mut column, &mut labels);

        // The empty ring lets the snapshot answer the boundary cull queries.
        let mut neighbours = Vec::with_capacity(8);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx != 0 || dz != 0 {
                    let mut neighbour = ChunkColumn::new(ChunkPos::new(dx, dz));
                    neighbour.fill_biomes(PLAINS);
                    neighbours.push(Arc::new(neighbour));
                }
            }
        }
        Self {
            center: Arc::new(column),
            neighbours,
            labels,
        }
    }

    /// Adds the M6 acceptance wing to the column (ADR-0008 §5 demo scene).
    ///
    /// Everything sits in the northern half and the south-east corner so the
    /// M5 rows (z = 3, 6, 9, 12) keep standing untouched; only stone support
    /// cells below the pool are replaced by water. The light writes follow the
    /// geometry so the mesher samples real nibbles: full sky on every
    /// non-opaque cell above the ground, darkness with a glowstone falloff in
    /// the cave.
    fn build_m6_wing(column: &mut ChunkColumn, labels: &mut Vec<&'static str>) {
        let stone = BlockState::new(1, 0);

        // 1. Flat sky-lit ground at y = 65, filling only the cells the M5
        //    rows and their supports do not occupy.
        for z in 0..16 {
            for x in 0..16 {
                if column.block(x, M6_GROUND_Y, z).is_air() {
                    column.set_block(x, M6_GROUND_Y, z, stone);
                }
            }
        }

        // 2. The L-shaped AO corner: a three-block L made of a two-block side
        //    wall along z at x = 1 (z = 0..1) and the wall along x at z = 1,
        //    both two blocks high. The inside corner at (2, 65, 0) opens
        //    north-east, toward the acceptance camera, so its ground quad
        //    shows the 15/10/5/0 AO gradient instead of being hidden behind
        //    the wall. The rest of the z = 1 wall carries the glass window.
        for y in M6_GROUND_Y + 1..=M6_WALL_TOP_Y {
            for z in 0..=1 {
                column.set_block(1, y, z, stone);
            }
            for x in 1..=9 {
                column.set_block(x, y, 1, stone);
            }
        }
        labels.push("AO corner wall");

        // 3. The 2×2 glass window in the side wall; glass is not opaque, so
        //    the corner keeps one single-occluder AO value of 10 next to it.
        for y in M6_GROUND_Y + 1..=M6_WALL_TOP_Y {
            for x in 4..=5 {
                column.set_block(x, y, 1, BlockState::new(20, 0));
            }
        }
        labels.push("glass window");

        // 4. Free-standing silhouette pillar at the north edge, clear of the
        //    M5 rows, so the day shot shows an unoccluded silhouette.
        for y in M6_GROUND_Y + 1..=M6_WALL_TOP_Y + 1 {
            column.set_block(11, y, 1, stone);
        }
        labels.push("silhouette pillar");

        // 5. The 3×3 water pool one block deep, directly in front of the
        //    glass window. The z = 0 edge faces the empty neighbour column,
        //    which also proves boundary faces are not culled.
        for z in 0..=2 {
            for x in 4..=6 {
                column.set_block(x, M6_GROUND_Y, z, BlockState::new(9, 0));
            }
        }
        labels.push("water pool");

        // 6. The 2×2 leaf canopy resting on the wall top, in front of the
        //    sky-lit stone face at x = 8..9.
        for z in 0..=1 {
            for x in 8..=9 {
                column.set_block(x, M6_WALL_TOP_Y + 1, z, BlockState::new(18, 0));
            }
        }
        labels.push("leaf canopy");

        // 6b. Liquid slopes along the south edge, levels written by hand
        //     because the demo world does not tick: water losing one level
        //     per block from a source at x = 15, lava two per block as in
        //     the overworld. The surfaces must step down and the flowing
        //     texture must turn west (`BlockFluidRenderer`).
        for (offset, level) in [0u8, 1, 2, 3, 4].into_iter().enumerate() {
            let id = if level == 0 { 9 } else { 8 };
            column.set_block(15 - offset, M6_GROUND_Y + 1, 14, BlockState::new(id, level));
        }
        labels.push("water slope");
        for (offset, level) in [0u8, 2, 4, 6].into_iter().enumerate() {
            let id = if level == 0 { 11 } else { 10 };
            column.set_block(15 - offset, M6_GROUND_Y + 1, 15, BlockState::new(id, level));
        }
        labels.push("lava slope");

        // 7. The cave: a stone shell under the platform with a 3×3×3 room,
        //    a glowstone in the west wall and a light falloff inside. The
        //    shell spans one cell beyond the interior on every side so the
        //    room is fully enclosed (the platform itself is the ceiling).
        for x in CAVE_GLOWSTONE[0]..=CAVE_MAX[0] {
            for y in CAVE_MIN[1] - 1..CAVE_MAX[1] {
                for z in CAVE_MIN[2] - 1..=CAVE_MAX[2] {
                    column.set_block(x, y, z, stone);
                }
            }
        }
        for x in CAVE_MIN[0]..CAVE_MAX[0] {
            for y in CAVE_MIN[1]..CAVE_MAX[1] {
                for z in CAVE_MIN[2]..CAVE_MAX[2] {
                    column.set_block(x, y, z, BlockState::AIR);
                }
            }
        }
        column.set_block(
            CAVE_GLOWSTONE[0],
            CAVE_GLOWSTONE[1],
            CAVE_GLOWSTONE[2],
            BlockState::new(89, 0),
        );
        labels.push("cave glowstone");

        // 8. Light. Full sky on every non-opaque cell above the ground: a
        //    section created by `set_block` carries no sky array, so without
        //    this write the whole wing would render dark. The cave is carved
        //    below y = 64 and keeps sky 0 with the glowstone falloff
        //    (14, then 12/10/8/6... by Manhattan distance), so it stays dark
        //    outside the falloff.
        for y in M6_GROUND_Y..80 {
            for z in 0..16 {
                for x in 0..16 {
                    if !column.block(x, y, z).is_opaque() {
                        column.set_sky_light(x, y, z, 15);
                    }
                }
            }
        }
        let [gx, gy, gz] = CAVE_GLOWSTONE;
        column.set_block_light(gx, gy, gz, 14);
        for x in CAVE_MIN[0]..CAVE_MAX[0] {
            for y in CAVE_MIN[1]..CAVE_MAX[1] {
                for z in CAVE_MIN[2]..CAVE_MAX[2] {
                    column.set_sky_light(x, y, z, 0);
                    let distance = (x.abs_diff(gx) + y.abs_diff(gy) + z.abs_diff(gz)) as u8;
                    let level = 14_u8.saturating_sub(2 * distance);
                    column.set_block_light(x, y, z, level);
                }
            }
        }
    }

    /// Meshes the centre column with the real model/atlas context.
    fn mesh(&self, context: &ModelMeshContext) -> Mesh {
        let columns =
            std::iter::once(Arc::clone(&self.center)).chain(self.neighbours.iter().map(Arc::clone));
        let snapshot = ChunkSnapshot::from_columns(DEMO_CHUNK, columns);
        mesh_chunk_with_models(DEMO_CHUNK, &snapshot, context)
    }
}

/// The stable acceptance report of one block demo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDemoReport {
    /// Framebuffer size used by the optional screenshot.
    pub viewport: (u32, u32),
    /// Counts of the loaded models and atlas.
    pub stats: ModelStats,
    /// World time in ticks.
    pub time: i64,
    /// Camera view name.
    pub view: &'static str,
    /// Quads emitted by the meshed demo column (opaque + transparent).
    pub quads: usize,
    /// Opaque quads (non-transparent faces).
    pub opaque_quads: usize,
    /// Transparent quads (water, glass, leaves).
    pub transparent_quads: usize,
    /// Vertices whose unpacked AO < 15.
    pub ao_shaded_vertices: usize,
    /// Vertices with sky light < 15 (cave checks).
    pub sky_vertices: usize,
    /// Vertices with block light > 0.
    pub block_lit_vertices: usize,
    /// Human-readable labels of every acceptance block in the scene.
    pub scene: Vec<&'static str>,
}

impl BlockDemoReport {
    /// Formats the stable acceptance report.
    ///
    /// The texture lines follow ADR-0007 exactly: block and item PNGs are
    /// reported with their `.mcmeta` files, and the layer count includes the
    /// original missing-texture checker.
    #[must_use]
    pub fn render(&self) -> String {
        let block = self.stats.block_textures;
        let item = self.stats.item_textures;
        let entries = block.total() + item.total();
        let png = block.png + item.png;
        let mcmeta = block.mcmeta + item.mcmeta;
        format!(
            "--- umicraft block demo ---\n\
             viewport          : {}x{}\n\
             blockstates       : {}\n\
             source models     : {}\n\
             block textures    : {} ({} png + {} mcmeta)\n\
             item textures     : {} ({} png + {} mcmeta)\n\
             texture entries   : {entries} ({png} png + {mcmeta} mcmeta)\n\
             texture layers    : {} ({} PNG frames + missing)\n\
             atlas             : {}x{} RGBA8, {} bytes\n\
             block states      : {}\n\
             biome tints       : {}\n\
             quads             : {}\n\
             opaque quads      : {}\n\
             transparent quads : {}\n\
             AO shaded verts   : {}\n\
             sky vertices      : {}\n\
             block lit verts   : {}\n\
             fallback variants : {}\n\
             time              : {}\n\
             view              : {}\n\
             scene blocks      : {}\n",
            self.viewport.0,
            self.viewport.1,
            self.stats.blockstates,
            self.stats.source_models,
            block.total(),
            block.png,
            block.mcmeta,
            item.total(),
            item.png,
            item.mcmeta,
            self.stats.layers,
            self.stats.layers.saturating_sub(1),
            self.stats.atlas_width,
            self.stats.atlas_height,
            group_digits(self.stats.atlas_bytes),
            self.stats.block_states,
            if self.stats.biome_tints { "on" } else { "off" },
            self.quads,
            self.opaque_quads,
            self.transparent_quads,
            self.ao_shaded_vertices,
            self.sky_vertices,
            self.block_lit_vertices,
            self.stats.fallback_variants,
            self.time,
            self.view,
            self.scene.join(", "),
        )
    }
}

/// Runs the block acceptance demo and writes the screenshot when requested.
///
/// # Errors
/// [`ClientError::ModelResources`] when the official installation, jar, models
/// or atlas are unavailable; [`ClientError::OfficialTexturesRejected`] when
/// the renderer rejects the atlas; or an offscreen render/readback/PNG error.
pub fn run(args: &Args) -> Result<(), ClientError> {
    let resources = ModelResources::load_with_pack_paths(&args.resource_packs)?;
    let scene = DemoScene::build();
    let mesh = scene.mesh(&resources.context());
    let report = build_report(args, &resources, &scene, &mesh);
    print!("{}", report.render());

    let Some(path) = args.screenshot.as_deref() else {
        return Ok(());
    };
    render_scene(args, &resources, &mesh, &report, path)?;
    println!(
        "screenshot written: {} ({}x{}, {} quads)",
        path.display(),
        args.width,
        args.height,
        report.quads
    );
    Ok(())
}

/// Builds the stable report from the meshed scene.
fn build_report(
    args: &Args,
    resources: &ModelResources,
    scene: &DemoScene,
    mesh: &Mesh,
) -> BlockDemoReport {
    // Vertex light counters across both passes.
    let mut ao_shaded = 0;
    let mut sky_lit_low = 0;
    let mut block_lit = 0;
    for vertex in mesh.vertices.iter().chain(&mesh.transparent_vertices) {
        let light = VertexLight::unpack(vertex.light);
        if light.ao < VertexLight::AO_OPEN {
            ao_shaded += 1;
        }
        if light.sky < 15 {
            sky_lit_low += 1;
        }
        if light.block > 0 {
            block_lit += 1;
        }
    }
    BlockDemoReport {
        viewport: (args.width, args.height),
        stats: resources.stats(),
        time: args.time,
        view: args.demo_view.as_str(),
        quads: mesh.face_count(),
        opaque_quads: mesh.opaque_face_count(),
        transparent_quads: mesh.transparent_face_count(),
        ao_shaded_vertices: ao_shaded,
        sky_vertices: sky_lit_low,
        block_lit_vertices: block_lit,
        scene: scene.labels.clone(),
    }
}

/// Renders the meshed scene and writes the PNG.
fn render_scene(
    args: &Args,
    resources: &ModelResources,
    mesh: &Mesh,
    report: &BlockDemoReport,
    path: &std::path::Path,
) -> Result<(), ClientError> {
    let mut renderer = prepare_renderer(args, resources, mesh)?;
    let image = draw_view(&mut renderer, args, report)?;
    write_png(path, &image.rgba, image.width, image.height)
}

/// Creates the offscreen renderer with the real atlas and the demo mesh.
fn prepare_renderer(
    args: &Args,
    resources: &ModelResources,
    mesh: &Mesh,
) -> Result<Renderer, ClientError> {
    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    // The demo mesh indexes the official atlas; there is no procedural
    // fallback for it (ADR-0012).
    if !TerrainTextures::official(resources)
        .install(&mut renderer)
        .is_official()
    {
        return Err(ClientError::OfficialTexturesRejected);
    }
    renderer.upload_mesh(DEMO_CHUNK, mesh);
    Ok(renderer)
}

/// Draws one acceptance view with the `--time` clock and reads the pixels.
fn draw_view(
    renderer: &mut Renderer,
    args: &Args,
    report: &BlockDemoReport,
) -> Result<umicraft_render::Image, ClientError> {
    // The renderer owns the sky/fog pass; the demo never writes a sky
    // texture. `--time` becomes the world clock of the environment.
    renderer.set_environment(RenderEnvironment::new(
        0,
        args.time,
        0.0,
        args.view_distance,
    ));

    let camera_spec = camera_for_view(args.demo_view);
    let mut camera = Camera::new(camera_spec.position, camera_spec.yaw, camera_spec.pitch);
    camera.set_aspect(args.width, args.height);
    let mut overlay = Overlay::new(args.width, args.height);
    let lines = [
        "UMICRAFT M6 BLOCK DEMO".to_string(),
        format!(
            "{} BLOCKSTATES  {} SOURCE MODELS",
            report.stats.blockstates, report.stats.source_models
        ),
        format!(
            "ATLAS {}x{}  {} LAYERS  {} BYTES",
            report.stats.atlas_width,
            report.stats.atlas_height,
            report.stats.layers,
            group_digits(report.stats.atlas_bytes)
        ),
        format!(
            "{} BLOCK STATES  {} QUADS  {} FALLBACKS",
            report.stats.block_states, report.quads, report.stats.fallback_variants
        ),
        format!(
            "TIME {}  VIEW {}  {} ACCEPTANCE BLOCKS",
            report.time,
            report.view,
            report.scene.len()
        ),
    ];
    draw_caption(&mut overlay, &lines, args.width, args.height);

    renderer.render(&camera, &overlay)?;
    Ok(renderer.read_pixels()?)
}

/// Returns the camera of one acceptance view.
///
/// Day and night share the ground camera; only `--time` differs.
fn camera_for_view(view: DemoView) -> DemoCamera {
    match view {
        DemoView::Overview => OVERVIEW_CAMERA,
        DemoView::Day | DemoView::Night => GROUND_CAMERA,
        DemoView::Cave => CAVE_CAMERA,
        DemoView::Transparency => TRANSPARENCY_CAMERA,
    }
}

/// Draws a stats panel in the corner, without the gameplay HUD.
///
/// The acceptance shot should show blocks, not a hotbar or status bars, but
/// the caption still uses [`HudLayout`] so it follows the same scale and
/// padding conventions as the in-game debug overlay.
fn draw_caption(overlay: &mut Overlay, lines: &[String], width: u32, height: u32) {
    const BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
    const TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    let layout = HudLayout::new(width, height);
    let scale = layout.scale();
    let text_width = lines
        .iter()
        .map(|line| Overlay::text_width(line, scale))
        .fold(0.0_f32, f32::max);
    let panel = layout.debug_panel_rect(text_width, lines.len());
    overlay.rect(panel.x, panel.y, panel.width, panel.height, BACKGROUND);
    for (index, line) in lines.iter().enumerate() {
        let (x, y) = layout.debug_text_origin(index);
        overlay.text(x, y, scale, TEXT, line);
    }
}

/// Groups decimal digits with commas, e.g. `2461696` -> `2,461,696`.
fn group_digits(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use umicraft_model::{
        AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource,
        StateDef,
    };

    use super::*;
    use crate::model_resources::TextureEntries;
    use umicraft_mesh::tint::BiomeTints;
    use umicraft_world::LightLevel;

    /// The exact ADR-0007 counts, independent of the machine's installation.
    fn fixture_stats() -> ModelStats {
        ModelStats {
            blockstates: 340,
            source_models: 1080,
            block_textures: TextureEntries {
                png: 373,
                mcmeta: 9,
            },
            item_textures: TextureEntries {
                png: 227,
                mcmeta: 2,
            },
            layers: 601,
            atlas_width: 32,
            atlas_height: 32,
            atlas_bytes: 2_461_696,
            block_states: 5_000,
            fallback_variants: 7,
            biome_tints: true,
        }
    }

    #[test]
    fn the_report_prints_the_adr_counts_and_scene_names() {
        let report = BlockDemoReport {
            viewport: (1280, 720),
            stats: fixture_stats(),
            time: 6000,
            view: "overview",
            quads: 1_337,
            opaque_quads: 1_300,
            transparent_quads: 37,
            ao_shaded_vertices: 24,
            sky_vertices: 108,
            block_lit_vertices: 36,
            scene: vec!["stone", "dirt", "oak stairs"],
        };
        let expected = "\
--- umicraft block demo ---
viewport          : 1280x720
blockstates       : 340
source models     : 1080
block textures    : 382 (373 png + 9 mcmeta)
item textures     : 229 (227 png + 2 mcmeta)
texture entries   : 611 (600 png + 11 mcmeta)
texture layers    : 601 (600 PNG frames + missing)
atlas             : 32x32 RGBA8, 2,461,696 bytes
block states      : 5000
biome tints       : on
quads             : 1337
opaque quads      : 1300
transparent quads : 37
AO shaded verts   : 24
sky vertices      : 108
block lit verts   : 36
fallback variants : 7
time              : 6000
view              : overview
scene blocks      : stone, dirt, oak stairs
";
        assert_eq!(report.render(), expected);
    }

    #[test]
    fn digit_grouping_is_stable() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_000), "1,000");
        assert_eq!(group_digits(2_461_696), "2,461,696");
    }

    #[test]
    fn the_scene_contains_every_acceptance_block() {
        let scene = DemoScene::build();
        let m6_labels = [
            "AO corner wall",
            "glass window",
            "silhouette pillar",
            "water pool",
            "leaf canopy",
            "water slope",
            "lava slope",
            "cave glowstone",
        ];
        let expected_labels = 52 + m6_labels.len();
        assert_eq!(scene.labels.len(), expected_labels);
        for name in [
            "stone",
            "dirt",
            "grass",
            "oak planks",
            "cobblestone",
            "sandstone",
            "bricks",
            "glass",
            "oak leaves",
            "water",
            "obsidian",
            "glowstone",
            "pumpkin",
            "melon",
            "netherrack",
            "quartz block",
        ] {
            assert!(scene.labels.contains(&name), "missing {name}");
        }
        for name in [
            "coal ore",
            "iron ore",
            "gold ore",
            "diamond ore",
            "redstone ore",
            "lapis ore",
            "emerald ore",
            "quartz ore",
        ] {
            assert!(scene.labels.contains(&name), "missing {name}");
        }
        for name in [
            "white wool",
            "orange wool",
            "magenta wool",
            "light blue wool",
            "yellow wool",
            "lime wool",
            "pink wool",
            "gray wool",
            "light gray wool",
            "cyan wool",
            "purple wool",
            "blue wool",
            "brown wool",
            "green wool",
            "red wool",
            "black wool",
        ] {
            assert!(scene.labels.contains(&name), "missing {name}");
        }
        for name in [
            "stone slab (bottom)",
            "stone slab (top)",
            "oak stairs",
            "oak fence",
            "glass pane",
            "torch",
            "red carpet",
            "snow layer",
            "cactus",
            "oak ladder",
            "rail",
            "vine",
        ] {
            assert!(scene.labels.contains(&name), "missing {name}");
        }
        for name in m6_labels {
            assert!(scene.labels.contains(&name), "missing M6 {name}");
        }

        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(scene.center.block(x, PLATFORM_Y, z), BlockState::new(1, 0));
            }
        }
        for (row_index, row) in ROWS.iter().enumerate() {
            let z = 12 - row_index * 3;
            for (index, demo) in row.iter().enumerate() {
                assert_eq!(
                    scene.center.block(1 + index, PLATFORM_Y + 1 + row_index, z),
                    demo.state,
                    "{} must stay at its M5 position",
                    demo.label
                );
            }
        }
    }

    #[test]
    fn the_m6_wing_geometry_and_light_are_in_place() {
        let scene = DemoScene::build();
        let column = &scene.center;
        let stone = BlockState::new(1, 0);

        // The ground fills y = 65 except where the M5 content stands.
        assert_eq!(column.block(0, M6_GROUND_Y, 0), stone);
        assert_eq!(column.block(15, M6_GROUND_Y, 15), stone);
        // Row 0 (y = 65, z = 12) is untouched by the ground fill.
        assert_eq!(column.block(2, M6_GROUND_Y, 12), BlockState::new(3, 0));

        // The L-shaped corner: the side wall along z at x = 1 (z = 0..1)
        // and the long wall along x at z = 1, both two blocks high. The
        // inside corner at (2, 65, 0) opens toward the north camera.
        for y in M6_GROUND_Y + 1..=M6_WALL_TOP_Y {
            assert_eq!(column.block(1, y, 0), stone, "side wall");
            assert_eq!(column.block(2, y, 1), stone, "wall B");
        }
        assert!(column.block(1, M6_GROUND_Y + 1, 2).is_air(), "open corner");
        assert!(column.block(2, M6_GROUND_Y + 3, 2).is_air(), "no roof");

        // The 2×2 glass window in the side wall.
        for y in M6_GROUND_Y + 1..=M6_WALL_TOP_Y {
            assert_eq!(column.block(4, y, 1), BlockState::new(20, 0), "glass");
            assert_eq!(column.block(5, y, 1), BlockState::new(20, 0), "glass");
        }

        // The silhouette pillar.
        for y in M6_GROUND_Y + 1..=M6_WALL_TOP_Y + 1 {
            assert_eq!(column.block(11, y, 1), stone, "pillar");
        }
        assert!(column.block(10, M6_GROUND_Y + 1, 1).is_air());

        // The 3×3 water pool one block deep.
        for z in 0..=2 {
            for x in 4..=6 {
                assert_eq!(column.block(x, M6_GROUND_Y, z), BlockState::new(9, 0));
            }
        }
        assert_eq!(column.block(7, M6_GROUND_Y, 1), stone);

        // The 2×2 leaf canopy on the wall top.
        for z in 0..=1 {
            for x in 8..=9 {
                assert_eq!(
                    column.block(x, M6_WALL_TOP_Y + 1, z),
                    BlockState::new(18, 0)
                );
            }
        }

        // The cave: air interior, stone shell, glowstone in the west wall.
        for x in CAVE_MIN[0]..CAVE_MAX[0] {
            for y in CAVE_MIN[1]..CAVE_MAX[1] {
                for z in CAVE_MIN[2]..CAVE_MAX[2] {
                    assert!(
                        column.block(x, y, z).is_air(),
                        "cave cell ({x},{y},{z}) must be air"
                    );
                }
            }
        }
        assert_eq!(
            column.block(CAVE_GLOWSTONE[0], CAVE_GLOWSTONE[1], CAVE_GLOWSTONE[2]),
            BlockState::new(89, 0)
        );
        // The shell encloses the room on every side: floor, east, north and
        // south walls, with the platform above as the ceiling.
        for (x, y, z, name) in [
            (5, 60, 6, "floor"),
            (4, 61, 6, "west wall"),
            (8, 62, 6, "east wall"),
            (5, 62, 4, "north wall"),
            (5, 62, 8, "south wall"),
        ] {
            assert_eq!(column.block(x, y, z), stone, "cave {name}");
        }
        assert_eq!(column.block(5, 64, 6), stone, "platform roof above cave");

        // Light: full sky above ground, darkness in the cave with the
        // 14/12/10/8 falloff around the glowstone.
        assert_eq!(column.light(8, 70, 8), LightLevel::FULL_SKY);
        assert_eq!(column.light(4, 62, 6), LightLevel::new(14, 0));
        assert_eq!(column.light(5, 62, 6), LightLevel::new(12, 0));
        assert_eq!(column.light(6, 62, 6), LightLevel::new(10, 0));
        assert_eq!(column.light(7, 62, 6), LightLevel::new(8, 0));
        for x in CAVE_MIN[0]..CAVE_MAX[0] {
            for y in CAVE_MIN[1]..CAVE_MAX[1] {
                for z in CAVE_MIN[2]..CAVE_MAX[2] {
                    let level = column.light(x, y, z);
                    assert_eq!(level.sky, 0, "cave cell ({x},{y},{z}) must be dark");
                }
            }
        }
    }

    /// One synthetic model element with all six faces on `texture`.
    fn element(from: [u8; 3], to: [u8; 3], texture: &str) -> String {
        format!(
            r#"{{"from":[{},{},{}],"to":[{},{},{}],"faces":{{"down":{{"texture":"{texture}"}},"up":{{"texture":"{texture}"}},"north":{{"texture":"{texture}"}},"south":{{"texture":"{texture}"}},"west":{{"texture":"{texture}"}},"east":{{"texture":"{texture}"}}}}}}"#,
            from[0], from[1], from[2], to[0], to[1], to[2]
        )
    }

    /// A model document made of synthetic elements.
    fn model(elements: &[String]) -> String {
        format!(r#"{{"elements":[{}]}}"#, elements.join(","))
    }

    /// An in-memory [`ResourceSource`] for the synthetic documents.
    #[derive(Default)]
    struct Memory {
        files: BTreeMap<String, Vec<u8>>,
    }

    impl Memory {
        fn model(mut self, name: &str, json: &str) -> Self {
            self.files.insert(
                format!("assets/minecraft/models/block/{name}.json"),
                json.as_bytes().to_vec(),
            );
            self
        }

        fn blockstate(mut self, name: &str, json: &str) -> Self {
            self.files.insert(
                format!("assets/minecraft/blockstates/{name}.json"),
                json.as_bytes().to_vec(),
            );
            self
        }
    }

    impl ResourceSource for Memory {
        fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| ModelError::Resource {
                    path: path.to_string(),
                    message: "synthetic resource is missing".to_string(),
                })
        }

        fn list(&self, prefix: &str) -> Vec<String> {
            self.files
                .keys()
                .filter(|path| path.starts_with(prefix))
                .cloned()
                .collect()
        }
    }

    const SLAB_STATES: &[StateDef] = &[
        StateDef {
            mask: 8,
            value: 0,
            blockstate: "synth_slab",
            variant: "bottom",
        },
        StateDef {
            mask: 8,
            value: 8,
            blockstate: "synth_slab",
            variant: "top",
        },
    ];
    const STAIR_STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "synth_stairs",
        variant: "normal",
    }];
    const FENCE_STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "synth_fence",
        variant: "normal",
    }];
    const TORCH_STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "synth_torch",
        variant: "normal",
    }];
    static SYNTH_DEFINITIONS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 44,
            name: "synth_slab",
            states: SLAB_STATES,
        },
        BlockDefinition {
            id: 53,
            name: "synth_stairs",
            states: STAIR_STATES,
        },
        BlockDefinition {
            id: 85,
            name: "synth_fence",
            states: FENCE_STATES,
        },
        BlockDefinition {
            id: 50,
            name: "synth_torch",
            states: TORCH_STATES,
        },
    ];

    fn blockstate(model: &str) -> String {
        format!(r#"{{"variants":{{"normal":{{"model":"{model}"}}}}}}"#)
    }

    fn slab_blockstate() -> String {
        r#"{"variants":{"bottom":{"model":"synth_slab_bottom"},"top":{"model":"synth_slab_top"}}}"#
            .to_string()
    }

    /// A synthetic context proving the mesher reads non-cube model geometry.
    fn synthetic_context() -> ModelMeshContext {
        let texture = "blocks/synth";
        let source = Memory::default()
            .model(
                "synth_slab_bottom",
                &model(&[element([0, 0, 0], [16, 8, 16], texture)]),
            )
            .model(
                "synth_slab_top",
                &model(&[element([0, 8, 0], [16, 16, 16], texture)]),
            )
            .model(
                "synth_stairs",
                &model(&[
                    element([0, 0, 0], [16, 8, 16], texture),
                    element([8, 8, 0], [16, 16, 16], texture),
                ]),
            )
            .model(
                "synth_fence",
                &model(&[
                    element([6, 0, 6], [10, 16, 10], texture),
                    element([0, 6, 7], [6, 9, 9], texture),
                    element([10, 6, 7], [16, 9, 9], texture),
                ]),
            )
            .model(
                "synth_torch",
                &model(&[element([7, 0, 7], [9, 10, 9], texture)]),
            )
            .blockstate("synth_slab", &slab_blockstate())
            .blockstate("synth_stairs", &blockstate("synth_stairs"))
            .blockstate("synth_fence", &blockstate("synth_fence"))
            .blockstate("synth_torch", &blockstate("synth_torch"));
        let models = BlockModelSet::build(&source, SYNTH_DEFINITIONS)
            .expect("the synthetic model set is valid");
        ModelMeshContext {
            models: Arc::new(models),
            atlas: Arc::new(AtlasLayout::from_keys([texture])),
            tints: None,
        }
    }

    /// Meshes one state alone at `(8, 8, 8)`.
    fn mesh_state(context: &ModelMeshContext, state: BlockState) -> Mesh {
        let mut column = ChunkColumn::new(DEMO_CHUNK);
        column.set_block(8, 8, 8, state);
        let snapshot = ChunkSnapshot::from_columns(DEMO_CHUNK, [Arc::new(column)]);
        mesh_chunk_with_models(DEMO_CHUNK, &snapshot, context)
    }

    fn assert_height(mesh: &Mesh, expected: f32, below: bool) {
        let epsilon = 1.0e-6;
        assert!(
            mesh.vertices
                .iter()
                .any(|vertex| (vertex.position[1] - expected).abs() < epsilon),
            "no vertex at height {expected}"
        );
        assert!(
            mesh.vertices.iter().all(|vertex| {
                if below {
                    vertex.position[1] <= expected + epsilon
                } else {
                    vertex.position[1] >= expected - epsilon
                }
            }),
            "a vertex crossed height {expected}"
        );
    }

    #[test]
    fn slab_stairs_fence_and_torch_models_emit_their_geometry() {
        let context = synthetic_context();

        let bottom = mesh_state(&context, BlockState::new(44, 0));
        assert_eq!(bottom.face_count(), 6);
        assert_height(&bottom, 8.5, true);

        let top = mesh_state(&context, BlockState::new(44, 8));
        assert_eq!(top.face_count(), 6);
        assert_height(&top, 8.5, false);

        assert!(mesh_state(&context, BlockState::new(53, 0)).face_count() > 6);
        assert!(mesh_state(&context, BlockState::new(85, 0)).face_count() > 6);
        assert!(mesh_state(&context, BlockState::new(50, 0)).face_count() > 0);
    }

    /// Collects the unpacked per-vertex light of both mesh passes.
    fn vertex_lights(mesh: &Mesh) -> Vec<VertexLight> {
        mesh.vertices
            .iter()
            .chain(&mesh.transparent_vertices)
            .map(|vertex| VertexLight::unpack(vertex.light))
            .collect()
    }

    #[test]
    fn the_demo_columns_are_plains() {
        let scene = DemoScene::build();
        assert!(scene.center.biomes().iter().all(|&biome| biome == PLAINS));
        for neighbour in &scene.neighbours {
            assert!(
                neighbour.biomes().iter().all(|&biome| biome == PLAINS),
                "the 3x3 ring must not blend the demo with ocean"
            );
        }
    }

    #[test]
    fn the_demo_leaves_and_water_use_the_biome_tints() {
        let mut context = synthetic_context();
        context.tints = Some(Arc::new(BiomeTints::from_samplers(
            |_, _| [0x79, 0xC0, 0x5A],
            |_, _| [0x59, 0xAE, 0x30],
        )));
        let scene = DemoScene::build();
        let mesh = scene.mesh(&context);

        // Leaves and vines fall back to the legacy cube in the synthetic
        // context and must still take the foliage colour.
        assert!(
            mesh.transparent_vertices
                .iter()
                .any(|vertex| vertex.tint == 0x59_AE_30),
            "the leaf canopy must be tinted with the foliage colour"
        );
        // Plains water has no override and must stay white.
        assert!(
            mesh.transparent_vertices
                .iter()
                .any(|vertex| vertex.tint == umicraft_mesh::tint::WHITE),
            "plains water must stay untinted"
        );
    }

    #[test]
    fn the_m6_wing_meshes_into_both_passes_with_ao_and_light() {
        // The synthetic context has models only for slabs, stairs, fences and
        // torches; the M6 water, glass and leaves fall back to the legacy
        // cube path, which is exactly the routing under test.
        let context = synthetic_context();
        let scene = DemoScene::build();
        let mesh = scene.mesh(&context);

        assert!(
            mesh.opaque_face_count() > 0,
            "the wing keeps an opaque pass"
        );
        assert!(
            mesh.transparent_face_count() > 0,
            "water, glass and leaves must reach the transparent pass"
        );

        let lights = vertex_lights(&mesh);
        let ao_values: Vec<u8> = {
            let mut values: Vec<u8> = lights.iter().map(|light| light.ao).collect();
            values.sort_unstable();
            values.dedup();
            values
        };
        // The inside corner of the L emits the full 1.8 AO quantization
        // (ADR-0008 §2): 15/10/5/0 across the corner quads.
        for expected in [0_u8, 5, 10, 15] {
            assert!(
                ao_values.contains(&expected),
                "the mesh must contain AO {expected}, got {ao_values:?}"
            );
        }

        assert!(
            lights.iter().any(|light| light.sky < 15),
            "the cave must contribute sky-dark vertices"
        );
        assert!(
            lights.iter().any(|light| light.block > 0),
            "the glowstone falloff must contribute block-lit vertices"
        );
        assert!(
            lights
                .iter()
                .any(|light| light.sky == 15 && light.block == 0),
            "the open ground must stay fully sky-lit"
        );
    }

    #[test]
    #[ignore = "requires the official 1.8.8 client jar"]
    fn the_official_counts_match_adr_0007() {
        let resources = ModelResources::load().expect("official resources");
        let stats = resources.stats();
        assert_eq!(stats.blockstates, 340);
        assert_eq!(stats.source_models, 1080);
        assert_eq!(stats.block_textures.png, 373);
        assert_eq!(stats.block_textures.mcmeta, 9);
        assert_eq!(stats.item_textures.png, 227);
        assert_eq!(stats.item_textures.mcmeta, 2);
        assert_eq!(stats.layers, 601);
        assert_eq!((stats.atlas_width, stats.atlas_height), (32, 32));
        assert_eq!(stats.atlas_bytes, 2_461_696);
        assert!(stats.block_states > 0);
        for row in ROWS {
            for demo in row {
                // Water has no blockstate definition in the frozen registry
                // (it stays a liquid on the legacy cube path); every other
                // acceptance block must resolve to a baked model.
                if demo.state.id() == 9 {
                    assert!(resources.models().model(demo.state).is_none());
                } else {
                    assert!(
                        resources.models().model(demo.state).is_some(),
                        "{} has no baked model",
                        demo.label
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "requires the official 1.8.8 client jar"]
    fn the_official_models_have_the_expected_shapes() {
        let resources = ModelResources::load().expect("official resources");
        let context = resources.context();

        let bottom = mesh_state(&context, BlockState::new(44, 0));
        assert_height(&bottom, 8.5, true);
        let top = mesh_state(&context, BlockState::new(44, 8));
        assert_height(&top, 8.5, false);
        assert!(mesh_state(&context, BlockState::new(53, 0)).face_count() > 6);
        // The official fence model is a narrow post: it still emits six quads
        // (one per face), so assert geometry exists rather than a face surplus.
        assert!(mesh_state(&context, BlockState::new(85, 0)).face_count() >= 6);
        assert!(mesh_state(&context, BlockState::new(50, 0)).face_count() > 0);
    }

    #[test]
    #[ignore = "requires a GPU adapter and the official 1.8.8 client jar"]
    fn the_block_demo_screenshot_writes_a_1280x720_png() {
        let path =
            std::env::temp_dir().join(format!("umicraft-block-demo-{}.png", std::process::id()));
        let words: Vec<String> = [
            "--block-demo",
            "--screenshot",
            path.to_str().expect("utf-8 temporary path"),
        ]
        .iter()
        .map(|word| (*word).to_string())
        .collect();
        let args = Args::parse(&words).expect("valid block demo arguments");
        run(&args).expect("the block demo renders");

        let decoder = png::Decoder::new(std::fs::File::open(&path).expect("screenshot exists"));
        let reader = decoder.read_info().expect("png header");
        assert_eq!(
            (reader.info().width, reader.info().height),
            (1280, 720),
            "the acceptance screenshot is 1280x720"
        );
        let bytes = std::fs::metadata(&path).expect("screenshot metadata").len();
        assert!(bytes > 8, "the PNG is not empty");
        std::fs::remove_file(&path).ok();
    }

    /// Mean luminance of a rectangle, on the 0..255 sRGB scale.
    ///
    /// The rec-601 weights keep the sky's blue and the stone's gray on one
    /// comparable scale; the absolute value does not matter, only the day,
    /// night and cave ordering does.
    fn mean_luminance(image: &umicraft_render::Image, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
        let mut sum = 0.0_f64;
        let mut count = 0_u64;
        for y in y0..y1.min(image.height) {
            for x in x0..x1.min(image.width) {
                let index = ((y * image.width + x) * 4) as usize;
                let red = f64::from(image.rgba[index]);
                let green = f64::from(image.rgba[index + 1]);
                let blue = f64::from(image.rgba[index + 2]);
                sum += red * 0.299 + green * 0.587 + blue * 0.114;
                count += 1;
            }
        }
        sum / f64::from(u32::try_from(count).unwrap_or(1).max(1))
    }

    #[test]
    #[ignore = "requires a GPU adapter and the official 1.8.8 client jar"]
    fn night_and_cave_acceptance_frames_are_darker_than_day() {
        let resources = ModelResources::load().expect("official resources");
        let scene = DemoScene::build();
        let mesh = scene.mesh(&resources.context());

        // A small 16:9 framebuffer keeps the llvmpipe run fast; the assertion
        // regions are relative, so the framing matches the acceptance shots.
        let mut renderer = match Renderer::new_offscreen(640, 360) {
            Ok(renderer) => renderer,
            Err(umicraft_render::RenderError::NoAdapter(error)) => {
                eprintln!(
                    "SKIP night_and_cave_acceptance_frames_are_darker_than_day: \
                     no wgpu adapter is available ({error})"
                );
                return;
            }
            Err(error) => panic!("the offscreen renderer is required: {error}"),
        };
        assert!(
            TerrainTextures::official(&resources)
                .install(&mut renderer)
                .is_official(),
            "the real atlas uploads"
        );
        renderer.upload_mesh(DEMO_CHUNK, &mesh);

        let mut frame = |view: &str, time: &str| {
            let words: Vec<String> = [
                "--block-demo".to_string(),
                "--demo-view".to_string(),
                view.to_string(),
                "--time".to_string(),
                time.to_string(),
                "--width".to_string(),
                "640".to_string(),
                "--height".to_string(),
                "360".to_string(),
            ]
            .to_vec();
            let args = Args::parse(&words).expect("valid acceptance arguments");
            let report = build_report(&args, &resources, &scene, &mesh);
            let image = draw_view(&mut renderer, &args, &report).expect("the view renders");
            (args, image)
        };

        let (_, day_image) = frame("day", "6000");
        let (_, night_image) = frame("day", "18000");
        let (_, cave_image) = frame("cave", "6000");

        // The lower-center band is terrain in the day/night ground camera and
        // cave wall in the cave camera; it avoids the caption panel and the
        // sky band.
        let region = |image: &umicraft_render::Image| {
            (
                image.width / 4,
                image.height / 2,
                image.width * 3 / 4,
                image.height * 7 / 8,
            )
        };
        let (x0, y0, x1, y1) = region(&day_image);
        let day = mean_luminance(&day_image, x0, y0, x1, y1);
        let night = mean_luminance(&night_image, x0, y0, x1, y1);
        let cave = mean_luminance(&cave_image, x0, y0, x1, y1);

        assert!(
            night < day * 0.5,
            "the night terrain must be much darker than the day one: \
             day {day:.1}, night {night:.1}"
        );
        assert!(
            cave < day * 0.75,
            "the cave frame must be darker than the day frame: \
             day {day:.1}, cave {cave:.1}"
        );
    }
}
