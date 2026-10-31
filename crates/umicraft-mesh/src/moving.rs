// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blocks in flight (P7d): the geometry of a block a piston is moving.
//!
//! The chunk mesh cannot hold these blocks, they change position every frame.
//! The client therefore appends each one to a small [`Mesh`] it rebuilds per
//! frame, like `TileEntityPistonRenderer` does with its own tessellator.
//!
//! The reference draws the model with `BlockModelRenderer.renderModel` and no
//! knowledge of the world beyond the light at the cell, so here:
//!
//! - every quad is emitted, `cullface` is ignored (the faces towards
//!   neighbouring blocks move away from them, so they must not vanish);
//! - the light of the cell is stored by the piston when the motion starts and
//!   applies to the whole block; the neighbours do not shade it (a one-block
//!   [`BlockSource`] stands in for the world);
//! - the model comes from the wire state alone. The piston head is looked up
//!   by key so the `short` property can be chosen per frame.
//!
//! References (ADR-0001): `TileEntityPistonRenderer.renderTileEntityAt`,
//! `BlockPistonExtension` (blockstate `facing`/`short`/`type`).

use umicraft_model::{BlockModel, ModelVariant};
use umicraft_world::{BlockState, LightLevel};

use crate::models::{emit_model_quads, select_variant, Placement};
use crate::{BlockSource, Mesh, ModelMeshContext};

/// A world made of a single block with a fixed light.
///
/// Placed at the origin; everything else is air, so no neighbour shades or
/// culls the moving block.
struct LoneBlock {
    state: BlockState,
    light: LightLevel,
}

impl BlockSource for LoneBlock {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        if [x, y, z] == [0, 0, 0] {
            self.state
        } else {
            BlockState::AIR
        }
    }

    fn light(&self, _x: i32, _y: i32, _z: i32) -> LightLevel {
        self.light
    }
}

/// Appends the model of `state` to `mesh`, drawn in the cell `position`
/// shifted by `offset`.
///
/// `position` only picks the weighted variant (so a moving block keeps the
/// look it had standing still); `offset` is the piston's interpolated
/// displacement. A state without a baked model adds nothing.
pub fn push_moving_block(
    mesh: &mut Mesh,
    state: BlockState,
    position: [i32; 3],
    offset: [f32; 3],
    light: LightLevel,
    context: &ModelMeshContext,
) {
    if let Some(model) = context.models.model(state) {
        push_model(mesh, model, state, position, offset, light, context);
    }
}

/// Appends a piston head to `mesh`, `facing` being the piston's direction.
///
/// `short` is the 1.8 blockstate property: the head is drawn shorter while the
/// arm is still hidden inside the base (`TileEntityPistonRenderer` sets it from
/// the progress). A direction or key with no baked model adds nothing.
pub fn push_piston_head(
    mesh: &mut Mesh,
    head: PistonHead,
    position: [i32; 3],
    offset: [f32; 3],
    light: LightLevel,
    context: &ModelMeshContext,
) {
    let Some(facing) = facing_name(head.facing) else {
        return;
    };
    let kind = if head.sticky { "sticky" } else { "normal" };
    let key = format!("facing={facing},short={},type={kind}", head.short);
    if let Some(model) = context.models.model_by_key("piston_head", &key) {
        // The head block id is what makes the faces opaque and untinted.
        let state = BlockState::new(PISTON_HEAD, 0);
        push_model(mesh, model, state, position, offset, light, context);
    }
}

/// Block id of `piston_head`.
const PISTON_HEAD: u16 = 34;

/// Which piston head to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PistonHead {
    /// The direction the piston faces, as a unit vector.
    pub facing: [i32; 3],
    /// A sticky piston's head (with the slime-coloured face).
    pub sticky: bool,
    /// The shortened head shown at the start of the stroke.
    pub short: bool,
}

/// `EnumFacing` name of a unit vector, as the blockstate documents spell it.
fn facing_name(direction: [i32; 3]) -> Option<&'static str> {
    match direction {
        [0, -1, 0] => Some("down"),
        [0, 1, 0] => Some("up"),
        [0, 0, -1] => Some("north"),
        [0, 0, 1] => Some("south"),
        [-1, 0, 0] => Some("west"),
        [1, 0, 0] => Some("east"),
        _ => None,
    }
}

fn push_model(
    mesh: &mut Mesh,
    model: &BlockModel,
    state: BlockState,
    position: [i32; 3],
    offset: [f32; 3],
    light: LightLevel,
    context: &ModelMeshContext,
) {
    if let Some(variant) = select_variant(model, position) {
        push_variant(mesh, variant, state, position, offset, light, context);
    }
}

/// Appends one chosen `variant`; the shared tail of the moving blocks and of
/// the flower pot plants, which both draw a model with no neighbour.
pub(crate) fn push_variant(
    mesh: &mut Mesh,
    variant: &ModelVariant,
    state: BlockState,
    position: [i32; 3],
    offset: [f32; 3],
    light: LightLevel,
    context: &ModelMeshContext,
) {
    let source = LoneBlock { state, light };
    let placement = Placement {
        position: [0, 0, 0],
        shift: [
            position[0] as f32 + offset[0],
            position[1] as f32 + offset[1],
            position[2] as f32 + offset[2],
        ],
        cull: false,
    };
    emit_model_quads(mesh, &source, context, state, variant, placement);
}
