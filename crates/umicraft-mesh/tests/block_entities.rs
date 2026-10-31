// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chests and signs reach the render thread as block-entity references and
//! leave no quad in the chunk mesh (P7c).

use std::sync::Arc;

use umicraft_mesh::{mesh_chunk_with_models, BlockSource, ModelMeshContext};
use umicraft_model::{AtlasLayout, BlockModelSet};
use umicraft_world::{BlockState, ChunkPos};

struct Scene(Vec<([i32; 3], BlockState)>);

impl BlockSource for Scene {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.0
            .iter()
            .find(|(position, _)| *position == [x, y, z])
            .map_or(BlockState::AIR, |(_, state)| *state)
    }
}

fn context() -> ModelMeshContext {
    ModelMeshContext {
        models: Arc::new(BlockModelSet::default()),
        atlas: Arc::new(AtlasLayout::from_keys(["blocks/stone"])),
        tints: None,
    }
}

#[test]
fn a_double_chest_is_two_references_and_no_geometry() {
    let chest = BlockState::new(54, 3);
    let scene = Scene(vec![([2, 5, 2], chest), ([3, 5, 2], chest)]);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &scene, &context());
    assert_eq!(
        mesh.face_count(),
        0,
        "a chest is not a missing-texture cube"
    );
    assert_eq!(mesh.block_entities.len(), 2);
    assert!(!mesh.is_empty(), "the chunk still has something to show");
    let west = mesh
        .block_entities
        .iter()
        .find(|entry| entry.position == [2, 5, 2])
        .expect("the western chest");
    assert!(west.same_neighbour.east && !west.same_neighbour.west);
}

#[test]
fn a_sign_is_a_reference() {
    let scene = Scene(vec![([1, 64, 1], BlockState::new(63, 4))]);
    let mesh = mesh_chunk_with_models(ChunkPos::new(0, 0), &scene, &context());
    assert_eq!(mesh.face_count(), 0);
    assert_eq!(mesh.block_entities.len(), 1);
}
