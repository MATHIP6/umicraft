// SPDX-License-Identifier: MIT OR Apache-2.0

//! The geometry of the blocks pistons are moving, rebuilt every frame (P7d).
//!
//! [`mesh_moving_blocks`] follows `TileEntityPistonRenderer.renderTileEntityAt`:
//!
//! - a block whose progress reached 1 is not drawn (the world block takes
//!   over);
//! - a piston head still in the first half of its stroke is drawn `short`;
//! - a retracting base that carries its head draws the head at the moving
//!   offset (`short` once past half way) and the base itself in place;
//! - anything else is its model shifted by the interpolated offset.

use umicraft_game::piston::{MovingBlock, MovingBlocks};
use umicraft_mesh::{push_moving_block, push_piston_head, Mesh, ModelMeshContext, PistonHead};

/// Block id of `sticky_piston`, whose head has the sticky face.
const STICKY_PISTON: u16 = 29;
/// Block id of `piston_head`.
const PISTON_HEAD: u16 = 34;
/// Metadata bit of a head that belongs to a sticky piston.
const HEAD_STICKY_BIT: u8 = 8;

/// Meshes every block in flight at `partial_tick`.
///
/// The vertices are in world coordinates, so the renderer draws the result
/// like a chunk. The mesh is empty when nothing moves.
#[must_use]
pub fn mesh_moving_blocks(
    blocks: &MovingBlocks,
    partial_tick: f32,
    context: &ModelMeshContext,
) -> Mesh {
    let mut mesh = Mesh::default();
    for block in blocks.blocks() {
        push_block(&mut mesh, block, partial_tick, context);
    }
    mesh
}

fn push_block(mesh: &mut Mesh, block: &MovingBlock, partial_tick: f32, context: &ModelMeshContext) {
    let progress = block.progress_at(partial_tick);
    if block.state.is_air() || progress >= 1.0 {
        return;
    }
    let offset = block.offset_at(partial_tick);
    let head = |sticky, short| PistonHead {
        facing: block.direction,
        sticky,
        short,
    };
    if block.state.id() == PISTON_HEAD && progress < 0.5 {
        let sticky = block.state.metadata() & HEAD_STICKY_BIT != 0;
        let head = head(sticky, true);
        push_piston_head(mesh, head, block.pos, offset, block.light, context);
    } else if block.show_head && !block.extending {
        let sticky = block.state.id() == STICKY_PISTON;
        let head = head(sticky, progress >= 0.5);
        push_piston_head(mesh, head, block.pos, offset, block.light, context);
        push_moving_block(mesh, block.state, block.pos, [0.0; 3], block.light, context);
    } else {
        push_moving_block(mesh, block.state, block.pos, offset, block.light, context);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use umicraft_model::{
        AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource,
        StateDef,
    };
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

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

    fn plate(height: u32) -> String {
        format!(
            r##"{{ "textures": {{ "particle": "#t", "t": "blocks/t" }},
  "elements": [{{ "from": [0, 0, 0], "to": [16, {height}, 16],
    "faces": {{ "up": {{ "texture": "#t" }} }} }}] }}"##
        )
    }

    /// Stone is a 16-pixel plate; the head is 12 pixels long, 4 when short;
    /// the sticky piston base is an 8-pixel plate.
    fn context() -> ModelMeshContext {
        const fn one(blockstate: &'static str, variant: &'static str) -> [StateDef; 1] {
            [StateDef {
                mask: 0,
                value: 0,
                blockstate,
                variant,
            }]
        }
        static STONE: [StateDef; 1] = one("stone", "normal");
        static BASE: [StateDef; 1] = one("sticky_piston", "extended=false,facing=up");
        static HEAD: [StateDef; 1] = one("piston_head", "facing=up,short=false,type=sticky");
        static DEFS: [BlockDefinition; 3] = [
            BlockDefinition {
                id: 1,
                name: "stone",
                states: &STONE,
            },
            BlockDefinition {
                id: 29,
                name: "sticky_piston",
                states: &BASE,
            },
            BlockDefinition {
                id: 34,
                name: "piston_head",
                states: &HEAD,
            },
        ];
        let mut files = Memory::default();
        let mut put = |path: &str, json: String| files.0.insert(path.into(), json.into_bytes());
        for (name, height) in [("full", 16), ("half", 8), ("long", 12), ("short", 4)] {
            put(
                &format!("assets/minecraft/models/block/{name}.json"),
                plate(height),
            );
        }
        put(
            "assets/minecraft/blockstates/stone.json",
            r#"{ "variants": { "normal": { "model": "full" } } }"#.into(),
        );
        put(
            "assets/minecraft/blockstates/sticky_piston.json",
            r#"{ "variants": { "extended=false,facing=up": { "model": "half" } } }"#.into(),
        );
        put(
            "assets/minecraft/blockstates/piston_head.json",
            r#"{ "variants": {
                "facing=up,short=false,type=sticky": { "model": "long" },
                "facing=up,short=true,type=sticky": { "model": "short" }
            } }"#
                .into(),
        );
        let set = BlockModelSet::build(&files, &DEFS).expect("synthetic pistons");
        ModelMeshContext {
            models: Arc::new(set),
            atlas: Arc::new(AtlasLayout::from_keys(["blocks/t"])),
            tints: None,
        }
    }

    fn moving(state: BlockState, extending: bool, show_head: bool, progress: f32) -> MovingBlock {
        MovingBlock {
            pos: [0, 10, 0],
            state,
            direction: [0, 1, 0],
            extending,
            show_head,
            progress,
            last_progress: progress,
            light: LightLevel::FULL_SKY,
        }
    }

    fn mesh_of(block: MovingBlock) -> Mesh {
        let mut blocks = MovingBlocks::default();
        blocks.push(block);
        mesh_moving_blocks(&blocks, 1.0, &context())
    }

    #[test]
    fn a_pushed_block_is_shifted_by_the_interpolated_offset() {
        let mesh = mesh_of(moving(BlockState::new(1, 0), true, false, 0.5));
        assert_eq!(mesh.opaque_face_count(), 1);
        assert_eq!(mesh.vertices[0].position[1], 10.0 - 0.5 + 1.0);
    }

    #[test]
    fn a_finished_or_empty_block_is_not_drawn() {
        assert!(mesh_of(moving(BlockState::new(1, 0), true, false, 1.0)).is_empty());
        assert!(mesh_of(moving(BlockState::AIR, true, false, 0.5)).is_empty());
        assert!(mesh_moving_blocks(&MovingBlocks::default(), 0.0, &context()).is_empty());
    }

    #[test]
    fn a_head_is_short_in_the_first_half_of_its_stroke() {
        let head = BlockState::new(34, 9);
        // The model's top is at height / 16 of the block, shifted by the offset.
        let early = mesh_of(moving(head, true, false, 0.25));
        assert_eq!(early.vertices[0].position[1], 10.0 + (0.25 - 1.0) + 0.25);
        let late = mesh_of(moving(head, true, false, 0.75));
        assert_eq!(late.vertices[0].position[1], 10.0 + (0.75 - 1.0) + 0.75);
    }

    #[test]
    fn a_retracting_base_draws_its_head_and_itself_in_place() {
        let base = BlockState::new(29, 1);
        let mesh = mesh_of(moving(base, false, true, 0.25));
        assert_eq!(mesh.opaque_face_count(), 2);
        // The head slides (long until half way: 12 px), the base stays put (8 px).
        assert_eq!(mesh.vertices[0].position[1], 10.0 + 0.75 + 0.75);
        assert_eq!(mesh.vertices[4].position[1], 10.5);
    }
}
