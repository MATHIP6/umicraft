// SPDX-License-Identifier: MIT OR Apache-2.0

//! The server-less M5 acceptance run (`--block-demo`).
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
//! vine. The real textures and geometry come from the installation at runtime;
//! no Mojang asset is copied into the repository.

use std::sync::Arc;

use glam::Vec3;
use umicraft_mesh::{mesh_chunk_with_models, ChunkSnapshot, Mesh, ModelMeshContext};
use umicraft_render::{Camera, HudLayout, Overlay, Renderer};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

use crate::args::Args;
use crate::error::ClientError;
use crate::model_resources::{ModelResources, ModelStats};
use crate::screenshot::write_png;

/// The single meshed column of the demo.
const DEMO_CHUNK: ChunkPos = ChunkPos::new(0, 0);
/// The full stone platform sits on this layer.
const PLATFORM_Y: usize = 64;
/// The camera height, chosen to see all four stepped rows.
const CAMERA_Y: f32 = 72.0;
/// The camera distance behind the platform.
const CAMERA_Z: f32 = 24.0;
/// The downward camera pitch of the acceptance shot.
const CAMERA_PITCH: f32 = -0.35;

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

        // The empty ring lets the snapshot answer the boundary cull queries.
        let mut neighbours = Vec::with_capacity(8);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx != 0 || dz != 0 {
                    neighbours.push(Arc::new(ChunkColumn::new(ChunkPos::new(dx, dz))));
                }
            }
        }
        Self {
            center: Arc::new(column),
            neighbours,
            labels,
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
    /// Quads emitted by the meshed demo column.
    pub quads: usize,
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
             quads             : {}\n\
             fallback variants : {}\n\
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
            self.quads,
            self.stats.fallback_variants,
            self.scene.join(", "),
        )
    }
}

/// Runs the block acceptance demo and writes the screenshot when requested.
///
/// # Errors
/// [`ClientError::ModelResources`] when the official installation, jar, models
/// or atlas are unavailable; [`ClientError::BlockAtlas`] when the renderer
/// rejects the atlas; or an offscreen render/readback/PNG error.
pub fn run(args: &Args) -> Result<(), ClientError> {
    let resources = ModelResources::load()?;
    let scene = DemoScene::build();
    let mesh = scene.mesh(&resources.context());
    let report = BlockDemoReport {
        viewport: (args.width, args.height),
        stats: resources.stats(),
        quads: mesh.face_count(),
        scene: scene.labels.clone(),
    };
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

/// Renders the meshed scene and writes the PNG.
fn render_scene(
    args: &Args,
    resources: &ModelResources,
    mesh: &Mesh,
    report: &BlockDemoReport,
    path: &std::path::Path,
) -> Result<(), ClientError> {
    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let atlas = resources.atlas();
    renderer.set_block_atlas(
        atlas.width(),
        atlas.height(),
        atlas.layers(),
        atlas.pixels(),
    )?;
    renderer.upload_mesh(DEMO_CHUNK, mesh);

    let mut camera = Camera::new(Vec3::new(8.0, CAMERA_Y, CAMERA_Z), 0.0, CAMERA_PITCH);
    camera.set_aspect(args.width, args.height);
    let mut overlay = Overlay::new(args.width, args.height);
    let lines = [
        "UMICRAFT M5 BLOCK DEMO".to_string(),
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
            "{} ACCEPTANCE BLOCKS: CUBES, ORES, WOOL, SHAPES",
            report.scene.len()
        ),
    ];
    draw_caption(&mut overlay, &lines, args.width, args.height);

    renderer.render(&camera, &overlay)?;
    let image = renderer.read_pixels()?;
    write_png(path, &image.rgba, image.width, image.height)
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
        }
    }

    #[test]
    fn the_report_prints_the_adr_counts_and_scene_names() {
        let report = BlockDemoReport {
            viewport: (1280, 720),
            stats: fixture_stats(),
            quads: 1_337,
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
quads             : 1337
fallback variants : 7
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
        assert_eq!(scene.labels.len(), 52);
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
                    demo.state
                );
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
}
