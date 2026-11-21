// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests of custom block colours in the model mesher. The models, the
//! atlas keys and the palettes are synthetic and generated in code.

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_mesh::custom_colors::CustomColors;
use umicraft_mesh::{mesh_chunk_with_settings, BlockSource, Mesh, MeshSettings, ModelMeshContext};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_packext::colors::Palette;
use umicraft_packext::ctm::BlockMatch;
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

fn context(custom: Option<CustomColors>) -> ModelMeshContext {
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
    ModelMeshContext {
        models: Arc::new(models),
        atlas: Arc::new(AtlasLayout::from_keys(["blocks/stone"])),
        tints: None,
        ctm: None,
        natural: None,
        custom_colors: custom.map(Arc::new),
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

fn mesh(context: &ModelMeshContext) -> Mesh {
    mesh_chunk_with_settings(
        ChunkPos::new(0, 0),
        &Single,
        context,
        MeshSettings::default(),
    )
}

#[test]
fn without_a_palette_the_block_stays_white() {
    let mesh = mesh(&context(None));
    assert_eq!(mesh.vertices.len(), 24);
    assert!(mesh
        .vertices
        .iter()
        .all(|v| v.tint == umicraft_mesh::tint::WHITE));
}

#[test]
fn a_palette_tints_every_face_of_its_block() {
    let mut custom = CustomColors::default();
    let stone = vec![BlockMatch {
        id: STONE_ID,
        metadata: None,
    }];
    custom.add_block_palette(stone, &Palette::fixed([0x12, 0x34, 0x56]));
    let mesh = mesh(&context(Some(custom)));
    assert_eq!(mesh.vertices.len(), 24);
    assert!(mesh.vertices.iter().all(|v| v.tint == 0x0012_3456));
}
