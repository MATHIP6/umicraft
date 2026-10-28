// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the M5 model mesher
//! ([`umicraft_mesh::mesh_chunk_with_models`]).
//!
//! The model sets are built from synthetic JSON held in memory and the atlas
//! is a plain [`AtlasLayout`], so the tests are deterministic and need neither
//! a Minecraft installation nor a Mojang asset (ADR-0007, spec §3).

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_mesh::tint::{BiomeTints, WHITE};
use umicraft_mesh::{
    mesh_chunk, mesh_chunk_with_models, BlockSource, ChunkSnapshot, Mesh, ModelMeshContext, Vertex,
    VertexLight,
};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

/// A full cube with `cullface` on every face (`block/cube_all` shape);
/// `#all` is replaced by the concrete texture key.
const CUBE_JSON: &str = r##"{
  "textures": { "particle": "#all", "all": "#all" },
  "elements": [{
    "from": [0, 0, 0],
    "to": [16, 16, 16],
    "faces": {
      "down":  { "texture": "#all", "cullface": "down" },
      "up":    { "texture": "#all", "cullface": "up" },
      "north": { "texture": "#all", "cullface": "north" },
      "south": { "texture": "#all", "cullface": "south" },
      "west":  { "texture": "#all", "cullface": "west" },
      "east":  { "texture": "#all", "cullface": "east" }
    }
  }]
}"##;

/// The bottom half of a cube (`block/half_slab_*` shape), no `cullface`.
const SLAB_JSON: &str = r##"{
  "textures": { "particle": "#all", "all": "#all" },
  "elements": [{
    "from": [0, 0, 0],
    "to": [16, 8, 16],
    "faces": {
      "down":  { "texture": "#all" },
      "up":    { "texture": "#all" },
      "north": { "texture": "#all" },
      "south": { "texture": "#all" },
      "west":  { "texture": "#all" },
      "east":  { "texture": "#all" }
    }
  }]
}"##;

/// The `block/cross` plant shape: two diagonal planes with four quads and no
/// `cullface`.
const CROSS_JSON: &str = r##"{  "textures": { "particle": "#cross", "cross": "#cross" },
  "elements": [
    {
      "from": [0.8, 0, 8],
      "to": [15.2, 16, 8],
      "rotation": { "origin": [8, 8, 8], "axis": "y", "angle": 45, "rescale": true },
      "shade": false,
      "faces": {
        "north": { "uv": [0, 0, 16, 16], "texture": "#cross" },
        "south": { "uv": [0, 0, 16, 16], "texture": "#cross" }
      }
    },
    {
      "from": [8, 0, 0.8],
      "to": [8, 16, 15.2],
      "rotation": { "origin": [8, 8, 8], "axis": "y", "angle": 45, "rescale": true },
      "shade": false,
      "faces": {
        "west": { "uv": [0, 0, 16, 16], "texture": "#cross" },
        "east": { "uv": [0, 0, 16, 16], "texture": "#cross" }
      }
    }
  ]
}"##;

/// A full cube whose top face alone is tinted, like the grass block top.
const TINTED_TOP_JSON: &str = r##"{
  "textures": { "particle": "#all", "all": "#all" },
  "elements": [{
    "from": [0, 0, 0],
    "to": [16, 16, 16],
    "faces": {
      "down":  { "texture": "#all" },
      "up":    { "texture": "#all", "tintindex": 0 },
      "north": { "texture": "#all" },
      "south": { "texture": "#all" },
      "west":  { "texture": "#all" },
      "east":  { "texture": "#all" }
    }
  }]
}"##;

/// In-memory [`ResourceSource`] for the synthetic documents.
#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
}

impl Memory {
    fn new() -> Self {
        Self::default()
    }

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

const STONE_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "stone",
    variant: "normal",
}];
const GLASS_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "glass",
    variant: "normal",
}];
const SLAB_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "stone_slab",
    variant: "normal",
}];
const PLANT_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "tall_grass",
    variant: "normal",
}];
const WEIGHTED_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "weighted",
    variant: "normal",
}];

const STONE: BlockDefinition = BlockDefinition {
    id: 1,
    name: "stone",
    states: STONE_STATES,
};
const GLASS: BlockDefinition = BlockDefinition {
    id: 20,
    name: "glass",
    states: GLASS_STATES,
};
const SLAB: BlockDefinition = BlockDefinition {
    id: 44,
    name: "stone_slab",
    states: SLAB_STATES,
};
const PLANT: BlockDefinition = BlockDefinition {
    id: 31,
    name: "tall_grass",
    states: PLANT_STATES,
};
const WEIGHTED: BlockDefinition = BlockDefinition {
    id: 4,
    name: "weighted",
    states: WEIGHTED_STATES,
};
const GRASS_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "grass",
    variant: "normal",
}];
const GRASS: BlockDefinition = BlockDefinition {
    id: 2,
    name: "grass",
    states: GRASS_STATES,
};

static CUBE_DEFS: &[BlockDefinition] = &[STONE];
static GLASS_DEFS: &[BlockDefinition] = &[STONE, GLASS];
static PLANT_DEFS: &[BlockDefinition] = &[STONE, PLANT];
static SCENE_DEFS: &[BlockDefinition] = &[STONE, GLASS, SLAB, PLANT];
static GRASS_DEFS: &[BlockDefinition] = &[GRASS];

/// Blockstate JSON for a single unweighted variant.
fn blockstate(model: &str) -> String {
    format!(r#"{{ "variants": {{ "normal": {{ "model": "{model}" }} }} }}"#)
}

/// A block source with an explicit, small scene; everything else is air.
struct Scene {
    blocks: Vec<([i32; 3], BlockState)>,
    biome: Option<u8>,
}

impl Scene {
    fn new(blocks: &[([i32; 3], BlockState)]) -> Self {
        Self {
            blocks: blocks.to_vec(),
            biome: None,
        }
    }

    /// Gives every column of the scene one biome id (tint tests).
    fn in_biome(mut self, biome: u8) -> Self {
        self.biome = Some(biome);
        self
    }
}

impl BlockSource for Scene {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.blocks
            .iter()
            .find(|(position, _)| *position == [x, y, z])
            .map_or(BlockState::AIR, |(_, state)| *state)
    }

    fn biome(&self, _x: i32, _z: i32) -> Option<u8> {
        self.biome
    }
}

/// A source with exactly one non-air block, used by the position scans.
struct Single {
    position: [i32; 3],
    state: BlockState,
}

impl BlockSource for Single {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        if [x, y, z] == self.position {
            self.state
        } else {
            BlockState::AIR
        }
    }
}

/// A context whose atlas contains exactly `keys` plus the missing layer.
fn context(set: BlockModelSet, keys: &[&str]) -> ModelMeshContext {
    ModelMeshContext {
        models: Arc::new(set),
        atlas: Arc::new(AtlasLayout::from_keys(keys.iter().copied())),
        tints: None,
    }
}

/// The atlas layer of a key the synthetic context must contain.
fn layer(context: &ModelMeshContext, key: &str) -> u32 {
    context
        .atlas
        .layer(key)
        .expect("the synthetic atlas has the key")
}

/// Iterates the four vertices of every emitted quad, both passes included.
fn quads(mesh: &Mesh) -> impl Iterator<Item = &[Vertex]> {
    mesh.vertices
        .chunks_exact(4)
        .chain(mesh.transparent_vertices.chunks_exact(4))
}

/// Iterates every vertex of both passes.
fn all_vertices(mesh: &Mesh) -> impl Iterator<Item = &Vertex> {
    mesh.vertices.iter().chain(mesh.transparent_vertices.iter())
}

/// Number of quads with `normal` and `layer`; used to prove which faces survive
/// the cull rules.
fn quads_with(mesh: &Mesh, normal: [f32; 3], layer: u32) -> usize {
    quads(mesh)
        .filter(|quad| quad[0].normal == normal && quad[0].layer == layer)
        .count()
}

/// Asserts that every quad winds counter-clockwise seen from its normal.
fn assert_counter_clockwise(mesh: &Mesh) {
    for quad in quads(mesh) {
        let [a, b, c] = [quad[0].position, quad[1].position, quad[2].position];
        let edge_a = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let edge_b = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
            edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
            edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
        ];
        let normal = quad[0].normal;
        let dot = cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2];
        assert!(
            dot > 0.0,
            "quad at {a:?} winds against its normal {normal:?}"
        );
    }
}

fn stone_set() -> ModelMeshContext {
    let source = Memory::new()
        .model("stone", &CUBE_JSON.replace("#all", "blocks/stone"))
        .blockstate("stone", &blockstate("stone"));
    context(
        BlockModelSet::build(&source, CUBE_DEFS).expect("synthetic stone model set"),
        &["blocks/stone"],
    )
}

#[test]
fn a_cube_model_emits_six_faces_with_the_atlas_layer() {
    let context = stone_set();
    let stone_layer = layer(&context, "blocks/stone");
    let mesh = mesh_chunk_with_models(
        ChunkPos::new(0, 0),
        &Single {
            position: [8, 8, 8],
            state: BlockState::new(1, 0),
        },
        &context,
    );

    assert_eq!(mesh.face_count(), 6);
    assert_eq!(mesh.triangle_count(), 12);
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.indices.len(), 36);
    assert!(all_vertices(&mesh).all(|vertex| vertex.layer == stone_layer));
    // The quad corners are block-local, so the whole cube sits at 8..=9.
    assert!(all_vertices(&mesh).all(|vertex| {
        (8.0..=9.0).contains(&vertex.position[0])
            && (8.0..=9.0).contains(&vertex.position[1])
            && (8.0..=9.0).contains(&vertex.position[2])
    }));
    for normal in [
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
    ] {
        assert_eq!(quads_with(&mesh, normal, stone_layer), 1, "{normal:?}");
    }
    assert_counter_clockwise(&mesh);
}

fn chunk_column(
    pos: ChunkPos,
    block: (usize, usize, usize),
    state: BlockState,
) -> Arc<ChunkColumn> {
    let mut column = ChunkColumn::new(pos);
    column.set_block(block.0, block.1, block.2, state);
    Arc::new(column)
}

#[test]
fn a_cube_culls_across_the_chunk_boundary() {
    let context = stone_set();
    let stone = BlockState::new(1, 0);
    let centre = ChunkPos::new(0, 0);

    let alone = ChunkSnapshot::from_columns(centre, [chunk_column(centre, (0, 8, 0), stone)]);
    let mesh = mesh_chunk_with_models(centre, &alone, &context);
    assert_eq!(mesh.face_count(), 6);

    // The neighbour block lives in the western column and is not meshed; it
    // must still hide the west face of the block at x = 0.
    let with_neighbour = ChunkSnapshot::from_columns(
        centre,
        [
            chunk_column(ChunkPos::new(-1, 0), (15, 8, 0), stone),
            chunk_column(centre, (0, 8, 0), stone),
        ],
    );
    let mesh = mesh_chunk_with_models(centre, &with_neighbour, &context);
    assert_eq!(mesh.face_count(), 5);
    assert_eq!(
        quads_with(&mesh, [-1.0, 0.0, 0.0], layer(&context, "blocks/stone")),
        0
    );
}

#[test]
fn a_bottom_slab_stops_at_half_height() {
    let source = Memory::new()
        .model(
            "half_slab",
            &SLAB_JSON.replace("#all", "blocks/stone_slab_top"),
        )
        .blockstate("stone_slab", &blockstate("half_slab"));
    let context = context(
        BlockModelSet::build(&source, &[SLAB]).expect("synthetic slab model set"),
        &["blocks/stone_slab_top"],
    );
    let slab_layer = layer(&context, "blocks/stone_slab_top");
    let blocks = Scene::new(&[([0, 0, 0], BlockState::new(44, 0))]);

    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &blocks, &context);
    assert_eq!(mesh.face_count(), 6, "a slab model has six quads");
    assert!(all_vertices(&mesh).all(|vertex| vertex.position[1] <= 0.5 + 1.0e-6));
    assert!(all_vertices(&mesh).any(|vertex| (vertex.position[1] - 0.5).abs() < 1.0e-6));
    assert!(all_vertices(&mesh).all(|vertex| vertex.layer == slab_layer));
    assert_counter_clockwise(&mesh);

    // The legacy path skips slabs (Flat) — they only render via models.
    let legacy = mesh_chunk(ChunkPos::new(0, 0), &blocks);
    assert_eq!(legacy.face_count(), 0);
}

#[test]
fn a_cross_emits_four_quads_and_is_not_culled() {
    let source = Memory::new()
        .model("cross", &CROSS_JSON.replace("#cross", "blocks/plant"))
        .model("stone", &CUBE_JSON.replace("#all", "blocks/stone"))
        .blockstate("tall_grass", &blockstate("cross"))
        .blockstate("stone", &blockstate("stone"));
    let context = context(
        BlockModelSet::build(&source, PLANT_DEFS).expect("synthetic cross model set"),
        &["blocks/plant", "blocks/stone"],
    );
    let plant_layer = layer(&context, "blocks/plant");
    let stone_layer = layer(&context, "blocks/stone");
    let grass = BlockState::new(31, 0);

    let alone = Scene::new(&[([5, 20, 5], grass)]);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &alone, &context);
    assert_eq!(mesh.face_count(), 4);
    assert_eq!(mesh.triangle_count(), 8);
    assert!(all_vertices(&mesh).all(|vertex| vertex.layer == plant_layer));
    assert_counter_clockwise(&mesh);
    // The coarse kind is Flat, so only the model makes the plant visible.
    assert!(mesh_chunk(ChunkPos::new(0, 0), &alone).is_empty());

    // A neighbouring opaque cube does not remove a cross quad: it has no
    // cullface, and the stone keeps its face towards the see-through plant.
    let next_to_stone = Scene::new(&[([5, 20, 5], grass), ([6, 20, 5], BlockState::new(1, 0))]);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &next_to_stone, &context);
    assert_eq!(mesh.face_count(), 10);
    assert_eq!(
        all_vertices(&mesh)
            .filter(|vertex| vertex.layer == plant_layer)
            .count(),
        16
    );
    assert_eq!(
        all_vertices(&mesh)
            .filter(|vertex| vertex.layer == stone_layer)
            .count(),
        24
    );
    // Cross quads carry `shade = false`; AO stays open even though the
    // synthetic model leaves `ambientocclusion` at its default of true.
    let plant_lights: Vec<VertexLight> = all_vertices(&mesh)
        .filter(|vertex| vertex.layer == plant_layer)
        .map(|vertex| VertexLight::unpack(vertex.light))
        .collect();
    assert!(plant_lights
        .iter()
        .all(|light| light.ao == VertexLight::AO_OPEN));
    assert!(plant_lights.iter().all(|light| !light.shade));
}

#[test]
fn glass_culls_only_against_the_same_state() {
    let source = Memory::new()
        .model("glass", &CUBE_JSON.replace("#all", "blocks/glass"))
        .model("stone", &CUBE_JSON.replace("#all", "blocks/stone"))
        .blockstate("glass", &blockstate("glass"))
        .blockstate("stone", &blockstate("stone"));
    let context = context(
        BlockModelSet::build(&source, GLASS_DEFS).expect("synthetic glass model set"),
        &["blocks/glass", "blocks/stone"],
    );
    let glass_layer = layer(&context, "blocks/glass");
    let stone_layer = layer(&context, "blocks/stone");
    let glass = BlockState::new(20, 0);
    let stone = BlockState::new(1, 0);
    let chunk = ChunkPos::new(0, 0);

    let two_glass = Scene::new(&[([0, 0, 0], glass), ([1, 0, 0], glass)]);
    let mesh = mesh_chunk_with_models(chunk, &two_glass, &context);
    assert_eq!(mesh.face_count(), 10);
    assert_eq!(mesh.opaque_face_count(), 0);
    assert_eq!(mesh.transparent_face_count(), 10);
    // Both faces in the shared plane x = 1 are gone; the outer planes remain.
    assert_eq!(quads_with(&mesh, [-1.0, 0.0, 0.0], glass_layer), 1);
    assert_eq!(quads_with(&mesh, [1.0, 0.0, 0.0], glass_layer), 1);
    assert!(all_vertices(&mesh).all(|vertex| vertex.layer == glass_layer));

    let mixed = Scene::new(&[([0, 0, 0], glass), ([1, 0, 0], stone)]);
    let mesh = mesh_chunk_with_models(chunk, &mixed, &context);
    assert_eq!(mesh.face_count(), 11);
    assert_eq!(mesh.opaque_face_count(), 6, "the stone stays opaque");
    assert_eq!(
        mesh.transparent_face_count(),
        5,
        "glass goes to the second pass"
    );
    // The glass face against the opaque stone is culled; the stone face
    // against the see-through glass is not.
    assert_eq!(quads_with(&mesh, [1.0, 0.0, 0.0], glass_layer), 0);
    assert_eq!(quads_with(&mesh, [-1.0, 0.0, 0.0], glass_layer), 1);
    assert_eq!(quads_with(&mesh, [-1.0, 0.0, 0.0], stone_layer), 1);
}

#[test]
fn modelless_states_keep_the_legacy_cube_but_only_atlas_layers() {
    // An empty model set: every state takes the legacy cube geometry, but
    // the model path never borrows a procedural layer index (ADR-0012): with
    // an atlas that has only the checker, every face samples the checker.
    let source = Memory::new();
    let set = BlockModelSet::build(&source, &[]).expect("an empty model set");
    assert_eq!(set.state_count(), 0);
    let atlas = AtlasLayout::default();
    let missing = atlas.missing_layer();
    let context = ModelMeshContext {
        models: Arc::new(set),
        atlas: Arc::new(atlas),
        tints: None,
    };
    let blocks = Scene::new(&[
        ([0, 0, 0], BlockState::new(1, 0)),
        ([1, 0, 0], BlockState::new(1, 0)),
        ([0, 1, 0], BlockState::new(20, 0)),
        ([0, 2, 0], BlockState::new(500, 0)),
    ]);
    let chunk = ChunkPos::new(0, 0);

    let models = mesh_chunk_with_models(chunk, &blocks, &context);
    let legacy = mesh_chunk(chunk, &blocks);
    assert_eq!(models.face_count(), legacy.face_count(), "same geometry");
    assert!(!models.vertices.is_empty());
    assert!(models
        .vertices
        .iter()
        .chain(&models.transparent_vertices)
        .all(|vertex| vertex.layer == missing));
}

#[test]
fn modelless_water_samples_the_real_fluid_layers() {
    let set = BlockModelSet::build(&Memory::new(), &[]).expect("an empty model set");
    let context = context(set, &["blocks/water_still", "blocks/water_flow"]);
    let still = layer(&context, "blocks/water_still");
    let flow = layer(&context, "blocks/water_flow");
    let mesh = mesh_chunk_with_models(
        ChunkPos::new(0, 0),
        &Single {
            position: [8, 8, 8],
            state: BlockState::new(9, 0),
        },
        &context,
    );

    // MCP `BlockFluidRenderer.renderFluid`: a lone still source is its
    // surface and the surface's underside, its bottom, and four sides drawn
    // from both sides; the surface and the bottom sample the still texture,
    // the sides the flowing one.
    assert_eq!(mesh.face_count(), 11);
    assert_eq!(quads_with(&mesh, [0.0, 1.0, 0.0], still), 1);
    assert_eq!(quads_with(&mesh, [0.0, -1.0, 0.0], still), 2);
    for normal in [
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
    ] {
        assert_eq!(quads_with(&mesh, normal, flow), 2, "{normal:?}");
    }
}

#[test]
fn weighted_variants_are_deterministic_and_vary_by_position() {
    let source = Memory::new()
        .model("variant_a", &CUBE_JSON.replace("#all", "blocks/stone"))
        .model("variant_b", &CUBE_JSON.replace("#all", "blocks/dirt"))
        .blockstate(
            "weighted",
            r#"{ "variants": { "normal": [
                { "model": "variant_a", "weight": 1 },
                { "model": "variant_b", "weight": 3 }
            ] } }"#,
        );
    let context = context(
        BlockModelSet::build(&source, &[WEIGHTED]).expect("synthetic weighted model set"),
        &["blocks/dirt", "blocks/stone"],
    );
    let dirt_layer = layer(&context, "blocks/dirt");
    let stone_layer = layer(&context, "blocks/stone");
    assert_ne!(dirt_layer, stone_layer);
    let chunk = ChunkPos::new(0, 0);

    let mut seen = [false, false];
    for x in 0..16 {
        for z in 0..16 {
            let blocks = Single {
                position: [x, 0, z],
                state: BlockState::new(4, 0),
            };
            let first = mesh_chunk_with_models(chunk, &blocks, &context);
            let second = mesh_chunk_with_models(chunk, &blocks, &context);
            assert_eq!(first, second, "position ({x}, 0, {z}) changed between runs");
            assert_eq!(first.face_count(), 6);
            let selected_layer = first.vertices[0].layer;
            assert!(selected_layer == dirt_layer || selected_layer == stone_layer);
            let index = if selected_layer == dirt_layer { 1 } else { 0 };
            seen[index] = true;
        }
    }
    assert_eq!(seen, [true, true], "both weighted variants must be used");
}

#[test]
fn the_mesh_is_deterministic_across_runs() {
    let source = Memory::new()
        .model("stone", &CUBE_JSON.replace("#all", "blocks/stone"))
        .model("glass", &CUBE_JSON.replace("#all", "blocks/glass"))
        .model(
            "half_slab",
            &SLAB_JSON.replace("#all", "blocks/stone_slab_top"),
        )
        .model("cross", &CROSS_JSON.replace("#cross", "blocks/plant"))
        .blockstate("stone", &blockstate("stone"))
        .blockstate("glass", &blockstate("glass"))
        .blockstate("stone_slab", &blockstate("half_slab"))
        .blockstate("tall_grass", &blockstate("cross"));
    let context = context(
        BlockModelSet::build(&source, SCENE_DEFS).expect("synthetic scene model set"),
        &[
            "blocks/glass",
            "blocks/plant",
            "blocks/stone",
            "blocks/stone_slab_top",
        ],
    );
    let scene = Scene::new(&[
        ([0, 0, 0], BlockState::new(1, 0)),
        ([1, 0, 0], BlockState::new(20, 0)),
        ([2, 0, 0], BlockState::new(44, 0)),
        ([3, 0, 0], BlockState::new(31, 0)),
        ([0, 1, 0], BlockState::new(1, 0)),
    ]);
    let chunk = ChunkPos::new(0, 0);

    let first = mesh_chunk_with_models(chunk, &scene, &context);
    let second = mesh_chunk_with_models(chunk, &scene, &context);
    assert_eq!(first, second);
    assert!(!first.is_empty());
    assert_counter_clockwise(&first);
}

/// A context whose single cube model is the given JSON.
fn cube_context(model_json: &str) -> ModelMeshContext {
    let source = Memory::new()
        .model("stone", model_json)
        .blockstate("stone", &blockstate("stone"));
    context(
        BlockModelSet::build(&source, CUBE_DEFS).expect("synthetic cube model set"),
        &["blocks/stone"],
    )
}

/// A two-block scene where the second stone darkens one corner of the first.
fn corner_occluder_scene() -> Scene {
    let stone = BlockState::new(1, 0);
    Scene::new(&[([0, 0, 0], stone), ([1, 1, 0], stone)])
}

#[test]
fn a_model_without_ambient_occlusion_keeps_its_corners_open() {
    let chunk = ChunkPos::new(0, 0);
    let scene = corner_occluder_scene();

    // The default model flag lets the diagonal block darken a corner.
    let normal = cube_context(&CUBE_JSON.replace("#all", "blocks/stone"));
    let mesh = mesh_chunk_with_models(chunk, &scene, &normal);
    assert!(all_vertices(&mesh)
        .any(|vertex| VertexLight::unpack(vertex.light).ao < VertexLight::AO_OPEN));

    // `ambientocclusion: false` keeps every corner open but still shades.
    let no_ao = CUBE_JSON.replace("#all", "blocks/stone").replacen(
        '{',
        "{\n  \"ambientocclusion\": false,",
        1,
    );
    let context = cube_context(&no_ao);
    let mesh = mesh_chunk_with_models(chunk, &scene, &context);
    assert!(all_vertices(&mesh).all(|vertex| {
        let light = VertexLight::unpack(vertex.light);
        light.ao == VertexLight::AO_OPEN && light.shade
    }));
}

#[test]
fn a_quad_with_shade_false_keeps_its_corners_open() {
    let chunk = ChunkPos::new(0, 0);
    let scene = corner_occluder_scene();

    let normal = cube_context(&CUBE_JSON.replace("#all", "blocks/stone"));
    let mesh = mesh_chunk_with_models(chunk, &scene, &normal);
    assert!(all_vertices(&mesh)
        .any(|vertex| VertexLight::unpack(vertex.light).ao < VertexLight::AO_OPEN));

    // The element `shade: false` flag (crosses, plants) disables corner AO
    // while the flag itself is still carried to the shader.
    let no_shade = CUBE_JSON
        .replace("#all", "blocks/stone")
        .replace("\"from\":", "\"shade\": false, \"from\":");
    let context = cube_context(&no_shade);
    let mesh = mesh_chunk_with_models(chunk, &scene, &context);
    assert!(all_vertices(&mesh).all(|vertex| {
        let light = VertexLight::unpack(vertex.light);
        light.ao == VertexLight::AO_OPEN && !light.shade
    }));
}

/// Context for the tinted-top grass model, optionally with a colour table.
fn grass_context(tints: Option<BiomeTints>) -> ModelMeshContext {
    let source = Memory::new()
        .model(
            "grass",
            &TINTED_TOP_JSON.replace("#all", "blocks/grass_top"),
        )
        .blockstate("grass", &blockstate("grass"));
    ModelMeshContext {
        models: Arc::new(
            BlockModelSet::build(&source, GRASS_DEFS).expect("synthetic grass model set"),
        ),
        atlas: Arc::new(AtlasLayout::from_keys(["blocks/grass_top"])),
        tints: tints.map(Arc::new),
    }
}

/// Returns the four vertices of the quad whose normal is `normal`.
fn quad_with(mesh: &Mesh, normal: [f32; 3]) -> &[Vertex] {
    quads(mesh)
        .find(|quad| quad[0].normal == normal)
        .expect("the quad exists")
}

#[test]
fn a_tinted_face_carries_the_biome_colour() {
    let tints = BiomeTints::from_samplers(|_, _| [0x79, 0xC0, 0x5A], |_, _| [0x59, 0xAE, 0x30]);
    let context = grass_context(Some(tints));
    let scene = Scene::new(&[([0, 0, 0], BlockState::new(2, 0))]).in_biome(1);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &scene, &context);

    let top = quad_with(&mesh, [0.0, 1.0, 0.0]);
    assert!(top.iter().all(|vertex| vertex.tint == 0x79_C0_5A));
    // The side faces have no `tintindex` and must keep their texture colours.
    for normal in [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
        assert!(quad_with(&mesh, normal)
            .iter()
            .all(|vertex| vertex.tint == WHITE));
    }
}

#[test]
fn without_a_colormap_the_face_stays_white() {
    let context = grass_context(None);
    let scene = Scene::new(&[([0, 0, 0], BlockState::new(2, 0))]).in_biome(1);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &scene, &context);
    assert!(all_vertices(&mesh).all(|vertex| vertex.tint == WHITE));
}

#[test]
fn the_tint_is_averaged_over_the_three_by_three_biomes() {
    // Plains (id 1) samples [90, 90, 90], desert (id 2) samples black.
    let tints = BiomeTints::from_samplers(
        |temperature, _| {
            if temperature > 1.0 {
                [0, 0, 0]
            } else {
                [90, 90, 90]
            }
        },
        |_, _| [0, 0, 0],
    );
    let context = grass_context(Some(tints));

    struct Border;
    impl BlockSource for Border {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            if (x, y, z) == (0, 0, 0) {
                BlockState::new(2, 0)
            } else {
                BlockState::AIR
            }
        }

        fn biome(&self, x: i32, _z: i32) -> Option<u8> {
            Some(if x < 0 { 2 } else { 1 })
        }
    }

    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &Border, &context);
    // Three of the nine columns are desert: (6 * 90) / 9 = 60.
    assert!(quad_with(&mesh, [0.0, 1.0, 0.0])
        .iter()
        .all(|vertex| vertex.tint == 0x3C_3C_3C));
}

#[test]
fn the_water_fallback_uses_the_swamp_water_tint() {
    let set = BlockModelSet::build(&Memory::new(), &[]).expect("an empty model set");
    let tints = BiomeTints::from_samplers(|_, _| [0; 3], |_, _| [0; 3]);
    let context = ModelMeshContext {
        models: Arc::new(set),
        atlas: Arc::new(AtlasLayout::default()),
        tints: Some(Arc::new(tints)),
    };
    let water = [([0, 0, 0], BlockState::new(8, 0))];

    let plains = mesh_chunk_with_models(
        ChunkPos::new(0, 0),
        &Scene::new(&water).in_biome(1),
        &context,
    );
    // The surface carries the biome colour as is; the other faces are
    // darkened by the reference's per-face factor.
    let surface_tint = |mesh: &umicraft_mesh::Mesh| {
        mesh.transparent_vertices
            .iter()
            .find(|vertex| vertex.normal == [0.0, 1.0, 0.0])
            .map(|vertex| vertex.tint)
    };
    assert_eq!(surface_tint(&plains), Some(WHITE));

    let swamp = mesh_chunk_with_models(
        ChunkPos::new(0, 0),
        &Scene::new(&water).in_biome(6),
        &context,
    );
    assert_eq!(surface_tint(&swamp), Some(0xE0_FF_AE));
}
