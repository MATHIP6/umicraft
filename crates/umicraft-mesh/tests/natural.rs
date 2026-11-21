// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests of natural textures in the model mesher. The models, the
//! atlas keys and the rule file are synthetic and generated in code.

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_mesh::natural::NaturalTable;
use umicraft_mesh::{mesh_chunk_with_settings, BlockSource, Mesh, MeshSettings, ModelMeshContext};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_packext::natural::NaturalRules;
use umicraft_world::{BlockState, ChunkPos};

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
static DEFS: [BlockDefinition; 1] = [BlockDefinition {
    id: STONE_ID,
    name: "stone",
    states: &STONE_STATES,
}];

fn context(rules: &str) -> ModelMeshContext {
    let mut files = Memory::default();
    files.0.insert(
        "assets/minecraft/models/block/stone.json".to_string(),
        CUBE_JSON.replace("#all", "blocks/stone").into_bytes(),
    );
    files.0.insert(
        "assets/minecraft/blockstates/stone.json".to_string(),
        br#"{ "variants": { "normal": { "model": "stone" } } }"#.to_vec(),
    );
    let models = BlockModelSet::build(&files, &DEFS).expect("synthetic models");
    let atlas = AtlasLayout::from_keys(["blocks/stone"]);
    let rules = NaturalRules::parse(rules.as_bytes()).expect("synthetic rules");
    let natural = NaturalTable::build(&rules, &atlas).map(Arc::new);
    ModelMeshContext {
        models: Arc::new(models),
        atlas: Arc::new(atlas),
        tints: None,
        ctm: None,
        natural,
        custom_colors: None,
    }
}

struct Single;

impl BlockSource for Single {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        if [x, y, z] == [3, 5, 7] {
            BlockState::new(STONE_ID, 0)
        } else {
            BlockState::AIR
        }
    }
}

fn mesh(context: &ModelMeshContext, natural_textures: bool) -> Mesh {
    let settings = MeshSettings {
        natural_textures,
        ..MeshSettings::default()
    };
    mesh_chunk_with_settings(ChunkPos::new(0, 0), &Single, context, settings)
}

fn uvs(mesh: &Mesh) -> Vec<[f32; 2]> {
    mesh.vertices.iter().map(|vertex| vertex.uv).collect()
}

#[test]
fn off_or_without_a_rule_the_coordinates_are_the_models() {
    let plain = uvs(&mesh(&context(""), true));
    assert_eq!(plain.len(), 24);
    assert_eq!(uvs(&mesh(&context("stone=4F"), false)), plain);
    assert_eq!(uvs(&mesh(&context("dirt=4F"), true)), plain);
}

#[test]
fn a_rule_permutes_the_corners_of_each_face_without_changing_them() {
    let plain = mesh(&context(""), true);
    let natural = mesh(&context("stone=4F"), true);
    assert_ne!(uvs(&natural), uvs(&plain), "this position must change");
    for (a, b) in plain
        .vertices
        .chunks_exact(4)
        .zip(natural.vertices.chunks_exact(4))
    {
        let key = |quad: &[umicraft_mesh::Vertex]| {
            let mut corners: Vec<_> = quad.iter().map(|v| v.uv.map(f32::to_bits)).collect();
            corners.sort_unstable();
            corners
        };
        assert_eq!(key(a), key(b));
        for (before, after) in a.iter().zip(b) {
            assert_eq!(before.position, after.position);
        }
    }
}

#[test]
fn the_same_position_always_gets_the_same_result() {
    let first = uvs(&mesh(&context("stone=4F"), true));
    assert_eq!(uvs(&mesh(&context("stone=4F"), true)), first);
}
