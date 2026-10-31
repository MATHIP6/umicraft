// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end check of the neighbour-dependent states (P7): the same fence
//! block meshes differently depending on what stands next to it.
//!
//! The blockstate is synthetic JSON held in memory (spec §3).

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_mesh::{mesh_chunk_with_models, BlockSource, ModelMeshContext};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_world::{BlockState, ChunkPos};

const FENCE: u16 = 85;
const STONE: u16 = 1;

/// A one-quad element facing up, `height` pixels tall.
fn plate(height: u32) -> String {
    format!(
        r##"{{ "textures": {{ "particle": "#t", "t": "blocks/t" }},
  "elements": [{{ "from": [0, 0, 0], "to": [16, {height}, 16],
    "faces": {{ "up": {{ "texture": "#t" }} }} }}] }}"##
    )
}

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

struct Scene(Vec<([i32; 3], BlockState)>);

impl BlockSource for Scene {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.0
            .iter()
            .find(|(position, _)| *position == [x, y, z])
            .map_or(BlockState::AIR, |(_, state)| *state)
    }
}

/// A fence whose "east connected" variant is a 4-pixel plate and whose other
/// variant is an 8-pixel plate.
fn fence_context() -> ModelMeshContext {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "fence",
        variant: "east=false,north=false,south=false,west=false",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: FENCE,
        name: "fence",
        states: STATES,
    }];
    let mut files = Memory::default();
    let mut put = |path: String, json: String| files.0.insert(path, json.into_bytes());
    put("assets/minecraft/models/block/post.json".into(), plate(8));
    put("assets/minecraft/models/block/arm.json".into(), plate(4));
    put(
        "assets/minecraft/blockstates/fence.json".into(),
        r#"{ "variants": {
            "east=false,north=false,south=false,west=false": { "model": "post" },
            "east=true,north=false,south=false,west=false": { "model": "arm" }
        } }"#
            .into(),
    );
    let set = BlockModelSet::build(&files, DEFS).expect("synthetic fence");
    ModelMeshContext {
        models: Arc::new(set),
        atlas: Arc::new(AtlasLayout::from_keys(["blocks/t"])),
        tints: None,
    }
}

/// Height of the single quad of the fence at the origin.
fn fence_height(neighbours: &[([i32; 3], BlockState)]) -> f32 {
    let mut blocks = vec![([0, 0, 0], BlockState::new(FENCE, 0))];
    blocks.extend_from_slice(neighbours);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &Scene(blocks), &fence_context());
    let fence_quad = mesh
        .vertices
        .chunks_exact(4)
        .find(|quad| quad[0].position[0] < 1.0 && quad[0].position[1] > 0.0)
        .expect("the fence emits its quad");
    fence_quad[0].position[1]
}

#[test]
fn a_lone_fence_uses_the_unconnected_variant() {
    assert_eq!(fence_height(&[]), 0.5);
}

#[test]
fn a_solid_neighbour_switches_the_fence_to_the_connected_variant() {
    let stone = BlockState::new(STONE, 0);
    assert_eq!(fence_height(&[([1, 0, 0], stone)]), 0.25);
}

#[test]
fn a_neighbour_on_another_side_keeps_the_east_variant_off() {
    let stone = BlockState::new(STONE, 0);
    // No `north=true` variant is baked: the lookup misses and the wire-state
    // model (the unconnected look) is used instead of a hole.
    assert_eq!(fence_height(&[([0, 0, -1], stone)]), 0.5);
}
