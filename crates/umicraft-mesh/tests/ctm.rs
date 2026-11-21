// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests of connected textures in the model mesher. The models,
//! the atlas keys and the rule files are synthetic and generated in code.

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_mesh::ctm::CtmTable;
use umicraft_mesh::{
    mesh_chunk_with_settings, BlockSource, ConnectedTextures, Mesh, MeshSettings, ModelMeshContext,
    Vertex,
};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_packext::ctm::parse_rules;
use umicraft_world::{BlockState, ChunkPos};

const GLASS_ID: u16 = 20;
const STONE_ID: u16 = 1;

const CUBE_JSON: &str = r##"{
  "textures": { "particle": "#all", "all": "#all" },
  "elements": [{
    "from": [0, 0, 0], "to": [16, 16, 16],
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

#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);

impl ResourceSource for Memory {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| ModelError::Resource {
                path: path.to_string(),
                message: "synthetic resource is missing".to_string(),
            })
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.0
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect()
    }
}

const fn state(name: &'static str) -> [StateDef; 1] {
    [StateDef {
        mask: 0,
        value: 0,
        blockstate: name,
        variant: "normal",
    }]
}
static STONE_STATES: [StateDef; 1] = state("stone");
static GLASS_STATES: [StateDef; 1] = state("glass");
static DEFS: [BlockDefinition; 2] = [
    BlockDefinition {
        id: STONE_ID,
        name: "stone",
        states: &STONE_STATES,
    },
    BlockDefinition {
        id: GLASS_ID,
        name: "glass",
        states: &GLASS_STATES,
    },
];

fn ids(name: &str) -> Option<u16> {
    match name {
        "minecraft:glass" => Some(GLASS_ID),
        "minecraft:stone" => Some(STONE_ID),
        _ => None,
    }
}

/// The 47 CTM tile keys of the synthetic glass pack.
fn glass_tile(n: usize) -> String {
    format!("blocks/ctm/glass/{n}")
}

/// A context with stone and glass, the tiles in the atlas and `rules` parsed.
fn context(rule_files: &[(&str, &str)]) -> ModelMeshContext {
    let mut files = Memory::default();
    for (name, texture) in [("stone", "blocks/stone"), ("glass", "blocks/glass")] {
        files.0.insert(
            format!("assets/minecraft/models/block/{name}.json"),
            CUBE_JSON.replace("#all", texture).into_bytes(),
        );
        files.0.insert(
            format!("assets/minecraft/blockstates/{name}.json"),
            format!(r#"{{ "variants": {{ "normal": {{ "model": "{name}" }} }} }}"#).into_bytes(),
        );
    }
    let models = BlockModelSet::build(&files, &DEFS).expect("synthetic models");
    let mut keys = vec!["blocks/stone".to_string(), "blocks/glass".to_string()];
    keys.extend((0..47).map(glass_tile));
    keys.push("blocks/ctm/stone/0".to_string());
    let atlas = AtlasLayout::from_keys(keys);
    let files: Vec<(&str, &[u8])> = rule_files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    let rules = parse_rules(files, &ids);
    let ctm = CtmTable::build(&rules, &atlas, &|_| None).map(Arc::new);
    ModelMeshContext {
        models: Arc::new(models),
        atlas: Arc::new(atlas),
        tints: None,
        ctm,
    }
}

const GLASS_RULE: (&str, &str) = (
    "ctm/glass/glass.properties",
    "matchBlocks=glass\nmethod=ctm\ntiles=0-46\n",
);

struct Scene(Vec<([i32; 3], BlockState)>);

impl BlockSource for Scene {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.0
            .iter()
            .find(|(at, _)| *at == [x, y, z])
            .map_or(BlockState::AIR, |(_, state)| *state)
    }
}

fn glass(positions: &[[i32; 3]]) -> Scene {
    Scene(
        positions
            .iter()
            .map(|at| (*at, BlockState::new(GLASS_ID, 0)))
            .collect(),
    )
}

fn settings(mode: ConnectedTextures) -> MeshSettings {
    MeshSettings {
        connected_textures: mode,
        ..MeshSettings::default()
    }
}

fn mesh(scene: &Scene, context: &ModelMeshContext, mode: ConnectedTextures) -> Mesh {
    mesh_chunk_with_settings(ChunkPos::new(0, 0), scene, context, settings(mode))
}

/// The layer of the quad facing `normal` whose lowest corner is `corner`.
fn layer_at(mesh: &Mesh, normal: [f32; 3], corner: [f32; 3]) -> Option<u32> {
    let quads = mesh
        .vertices
        .chunks_exact(4)
        .chain(mesh.transparent_vertices.chunks_exact(4));
    quads
        .filter(|quad| quad[0].normal == normal)
        .find(|quad| {
            (0..3).all(|axis| {
                quad.iter()
                    .map(|v| v.position[axis])
                    .fold(f32::INFINITY, f32::min)
                    == corner[axis]
            })
        })
        .map(|quad| quad[0].layer)
}

fn all_vertices(mesh: &Mesh) -> Vec<Vertex> {
    mesh.vertices
        .iter()
        .chain(&mesh.transparent_vertices)
        .copied()
        .collect()
}

const SOUTH: [f32; 3] = [0.0, 0.0, 1.0];

#[test]
fn a_two_block_glass_wall_gets_the_edge_tiles() {
    let context = context(&[GLASS_RULE]);
    let wall = glass(&[[8, 8, 8], [9, 8, 8]]);
    let mesh = mesh(&wall, &context, ConnectedTextures::Fast);
    let tile = |n: usize| context.atlas.layer(&glass_tile(n));
    // South face: the block at x=8 has a neighbour on its right (tile 1),
    // the block at x=9 on its left (tile 3).
    assert_eq!(layer_at(&mesh, SOUTH, [8.0, 8.0, 9.0]), tile(1));
    assert_eq!(layer_at(&mesh, SOUTH, [9.0, 8.0, 9.0]), tile(3));
    // A lone block keeps tile 0.
    let single = mesh_chunk_with_settings(
        ChunkPos::new(0, 0),
        &glass(&[[8, 8, 8]]),
        &context,
        settings(ConnectedTextures::Fast),
    );
    assert_eq!(layer_at(&single, SOUTH, [8.0, 8.0, 9.0]), tile(0));
}

#[test]
fn fancy_draws_the_missing_corner_and_fast_does_not() {
    let context = context(&[GLASS_RULE]);
    // The block at (8,8,8) has neighbours east and above; the corner between
    // them (east+above) is air.
    let corner = glass(&[[8, 8, 8], [9, 8, 8], [8, 9, 8]]);
    let fast = mesh(&corner, &context, ConnectedTextures::Fast);
    let fancy = mesh(&corner, &context, ConnectedTextures::Fancy);
    let tile = |n: usize| context.atlas.layer(&glass_tile(n));
    assert_eq!(layer_at(&fast, SOUTH, [8.0, 8.0, 9.0]), tile(37));
    assert_eq!(layer_at(&fancy, SOUTH, [8.0, 8.0, 9.0]), tile(16));
}

#[test]
fn off_and_no_rules_leave_the_mesh_unchanged() {
    let wall = glass(&[[8, 8, 8], [9, 8, 8], [8, 9, 8]]);
    let with_rules = context(&[GLASS_RULE]);
    let without = context(&[]);
    assert!(without.ctm.is_none());
    let vanilla = all_vertices(&mesh(&wall, &without, ConnectedTextures::Fancy));
    assert_eq!(
        all_vertices(&mesh(&wall, &with_rules, ConnectedTextures::Off)),
        vanilla
    );
    // A rule for another block changes nothing either.
    let elsewhere = context(&[(
        "ctm/other.properties",
        "matchBlocks=stone\nmethod=fixed\ntiles=ctm/stone/0\n",
    )]);
    assert_eq!(
        all_vertices(&mesh(&wall, &elsewhere, ConnectedTextures::Fancy)),
        vanilla
    );
    assert_ne!(
        all_vertices(&mesh(&wall, &with_rules, ConnectedTextures::Fast)),
        vanilla
    );
}

#[test]
fn a_tile_rule_replaces_the_sprite_for_every_block_showing_it() {
    let context = context(&[(
        "ctm/stone.properties",
        "matchTiles=stone\nmethod=fixed\ntiles=ctm/stone/0\n",
    )]);
    let scene = Scene(vec![([8, 8, 8], BlockState::new(STONE_ID, 0))]);
    let mesh = mesh(&scene, &context, ConnectedTextures::Fast);
    let fixed = context
        .atlas
        .layer("blocks/ctm/stone/0")
        .expect("tile layer");
    assert!(all_vertices(&mesh)
        .iter()
        .all(|vertex| vertex.layer == fixed));
}

#[test]
fn rules_with_missing_tiles_are_dropped() {
    let context = context(&[(
        "ctm/glass/glass.properties",
        "matchBlocks=glass\nmethod=ctm\ntiles=0-50\n",
    )]);
    assert!(context.ctm.is_none(), "tiles 47 to 50 are not in the atlas");
}
