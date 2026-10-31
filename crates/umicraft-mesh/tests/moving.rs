// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blocks moved by a piston are meshed without culling, shifted by the
//! piston offset (P7d).
//!
//! The blockstates are synthetic JSON held in memory (spec §3).

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_mesh::{
    mesh_chunk_with_models, push_moving_block, push_piston_head, BlockSource, Mesh,
    ModelMeshContext, PistonHead,
};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_world::{BlockState, ChunkPos, LightLevel};

const STONE: u16 = 1;
const HEAD: u16 = 34;
const MOVING: u16 = 36;

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
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }
}

/// A one-quad model facing up, `height` pixels tall, with a cullface.
fn plate(height: u32) -> String {
    format!(
        r##"{{ "textures": {{ "particle": "#t", "t": "blocks/t" }},
  "elements": [{{ "from": [0, 0, 0], "to": [16, {height}, 16],
    "faces": {{ "up": {{ "texture": "#t", "cullface": "up" }} }} }}] }}"##
    )
}

fn context() -> ModelMeshContext {
    static STONE_STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "stone",
        variant: "normal",
    }];
    static HEAD_STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "piston_head",
        variant: "facing=up,short=false,type=normal",
    }];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: STONE,
            name: "stone",
            states: STONE_STATES,
        },
        BlockDefinition {
            id: HEAD,
            name: "piston_head",
            states: HEAD_STATES,
        },
    ];
    let mut files = Memory::default();
    let mut put = |path: &str, json: String| files.0.insert(path.into(), json.into_bytes());
    put("assets/minecraft/models/block/full.json", plate(16));
    put("assets/minecraft/models/block/long.json", plate(12));
    put("assets/minecraft/models/block/short.json", plate(4));
    put(
        "assets/minecraft/blockstates/stone.json",
        r#"{ "variants": { "normal": { "model": "full" } } }"#.into(),
    );
    put(
        "assets/minecraft/blockstates/piston_head.json",
        r#"{ "variants": {
            "facing=up,short=false,type=normal": { "model": "long" },
            "facing=up,short=true,type=normal": { "model": "short" }
        } }"#
            .into(),
    );
    let set = BlockModelSet::build(&files, DEFS).expect("synthetic pistons");
    ModelMeshContext {
        models: Arc::new(set),
        atlas: Arc::new(AtlasLayout::from_keys(["blocks/t"])),
        tints: None,
    }
}

fn moved_stone(position: [i32; 3], offset: [f32; 3], light: LightLevel) -> Mesh {
    let mut mesh = Mesh::default();
    let stone = BlockState::new(STONE, 0);
    push_moving_block(&mut mesh, stone, position, offset, light, &context());
    mesh
}

#[test]
fn the_faces_are_shifted_by_the_offset_from_the_cell() {
    let mesh = moved_stone([10, 64, -3], [0.0, -0.5, 0.0], LightLevel::FULL_SKY);
    assert_eq!(mesh.opaque_face_count(), 1);
    let top = mesh.vertices[0].position;
    assert_eq!(top[1], 64.5);
    assert!((10.0..=11.0).contains(&top[0]));
    assert!((-3.0..=-2.0).contains(&top[2]));
}

#[test]
fn a_moving_block_keeps_the_face_a_world_neighbour_would_cull() {
    // The same stone in a world with a stone above has its top culled.
    struct Stacked;
    impl BlockSource for Stacked {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            if (x, z) == (0, 0) && (0..2).contains(&y) {
                BlockState::new(STONE, 0)
            } else {
                BlockState::AIR
            }
        }
    }
    let world = mesh_chunk_with_models(ChunkPos::new(0, 0), &Stacked, &context());
    assert_eq!(world.opaque_face_count(), 1, "only the upper top face");
    let moving = moved_stone([0, 0, 0], [0.0; 3], LightLevel::FULL_SKY);
    assert_eq!(moving.opaque_face_count(), 1);
}

#[test]
fn the_light_of_the_cell_lights_the_block() {
    let dim = LightLevel { block: 3, sky: 0 };
    let lit = moved_stone([0, 0, 0], [0.0; 3], LightLevel::FULL_SKY);
    let dark = moved_stone([0, 0, 0], [0.0; 3], dim);
    assert_ne!(lit.vertices[0].light, dark.vertices[0].light);
}

fn head(short: bool) -> Mesh {
    let mut mesh = Mesh::default();
    let head = PistonHead {
        facing: [0, 1, 0],
        sticky: false,
        short,
    };
    push_piston_head(
        &mut mesh,
        head,
        [0, 0, 0],
        [0.0; 3],
        LightLevel::FULL_SKY,
        &context(),
    );
    mesh
}

#[test]
fn the_short_property_selects_the_short_head_model() {
    assert_eq!(head(false).vertices[0].position[1], 0.75);
    assert_eq!(head(true).vertices[0].position[1], 0.25);
}

#[test]
fn a_head_with_no_baked_model_adds_nothing() {
    let mut mesh = Mesh::default();
    let head = PistonHead {
        facing: [0, 1, 0],
        sticky: true,
        short: false,
    };
    push_piston_head(
        &mut mesh,
        head,
        [0, 0, 0],
        [0.0; 3],
        LightLevel::FULL_SKY,
        &context(),
    );
    assert!(mesh.is_empty());
}

#[test]
fn the_moving_piston_block_itself_is_invisible_in_the_chunk_mesh() {
    struct Moving;
    impl BlockSource for Moving {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            if [x, y, z] == [0, 0, 0] {
                BlockState::new(MOVING, 0)
            } else {
                BlockState::AIR
            }
        }
    }
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &Moving, &context());
    assert!(mesh.is_empty());
}
