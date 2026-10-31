// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chests (MCP `TileEntityChestRenderer`, `TileEntityEnderChestRenderer`).
//!
//! Two chests of the same block side by side are one double chest. The
//! reference draws it once, from the chest with no neighbour on its negative
//! (west or north) side, with the large model; the other half draws nothing.
//! Its lid angle is the one of that anchor chest, which is the one a
//! `Block Action` addresses for a pair opened together.

use std::collections::HashMap;

use umicraft_mesh::{BlockEntityKind, BlockEntityRef};
use umicraft_model::entity::{chest_facing_degrees, ChestModel, TextureSlot};
use umicraft_render::EntityVertex;

use super::{push_at_block, BlockEntityScene};

/// Draws the chest at `entry` into `batches`.
pub(super) fn add_chest(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let around = entry.same_neighbour;
    let is_anchor = !around.west && !around.north;
    if !is_anchor {
        return;
    }
    let kind = BlockEntityKind::of(entry.state);
    // An ender chest never merges: the mesher only compares equal block ids,
    // and it would have to be two ender chests, which vanilla draws apart.
    let large = kind != Some(BlockEntityKind::EnderChest) && (around.east || around.south);
    let [x, y, z] = entry.position;
    let angle = scene.lids.lid_angle((x, y, z), scene.partial_tick);
    let facing = chest_facing_degrees(entry.state.metadata());
    push_at_block(
        batches.entry(texture(kind, large)).or_default(),
        ChestModel::emit_at(angle, large, facing),
        entry,
    );
}

/// The texture sheet of a chest.
fn texture(kind: Option<BlockEntityKind>, large: bool) -> TextureSlot {
    match (kind, large) {
        (Some(BlockEntityKind::EnderChest), _) => TextureSlot::ChestEnder,
        (Some(BlockEntityKind::TrappedChest), false) => TextureSlot::ChestTrapped,
        (Some(BlockEntityKind::TrappedChest), true) => TextureSlot::ChestTrappedDouble,
        (_, false) => TextureSlot::Chest,
        (_, true) => TextureSlot::ChestDouble,
    }
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::SignTable;
    use umicraft_mesh::SameNeighbours;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    fn chest(id: u16, same: SameNeighbours) -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(id, 3),
            same_neighbour: same,
            light: LightLevel { block: 3, sky: 12 },
        }
    }

    fn draw(entry: &BlockEntityRef) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let lids = umicraft_game::server::chest_lids::ChestLids::default();
        let scene = BlockEntityScene {
            data: &umicraft_game::server::block_entities::EntityTable::new(),
            seconds: 0.0,
            lids: &lids,
            signs: &SignTable::new(),
            font: None,
            partial_tick: 0.0,
            camera: glam::Vec3::ZERO,
        };
        let mut batches = HashMap::new();
        add_chest(&mut batches, entry, &scene);
        batches
    }

    #[test]
    fn a_single_chest_uses_the_chest_sheet_inside_its_block() {
        let batches = draw(&chest(54, SameNeighbours::default()));
        let vertices = &batches[&TextureSlot::Chest];
        assert!(!vertices.is_empty());
        for vertex in vertices {
            let [x, y, z] = vertex.position;
            assert!((4.0..=5.0).contains(&x) && (70.0..=71.0).contains(&y));
            assert!((9.0..=10.0).contains(&z));
            assert_eq!(vertex.light, 3 | (12 << 4));
        }
    }

    #[test]
    fn only_the_anchor_of_a_double_chest_draws_and_uses_the_large_sheet() {
        let anchor = SameNeighbours {
            east: true,
            ..SameNeighbours::default()
        };
        let other = SameNeighbours {
            west: true,
            ..SameNeighbours::default()
        };
        assert!(draw(&chest(54, anchor)).contains_key(&TextureSlot::ChestDouble));
        assert!(draw(&chest(54, other)).is_empty());
    }

    #[test]
    fn trapped_and_ender_chests_have_their_own_sheets() {
        assert!(
            draw(&chest(146, SameNeighbours::default())).contains_key(&TextureSlot::ChestTrapped)
        );
        let east = SameNeighbours {
            east: true,
            ..SameNeighbours::default()
        };
        assert!(draw(&chest(146, east)).contains_key(&TextureSlot::ChestTrappedDouble));
        assert!(draw(&chest(130, east)).contains_key(&TextureSlot::ChestEnder));
    }
}
