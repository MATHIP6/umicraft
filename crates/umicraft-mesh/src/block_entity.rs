// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blocks drawn by a tile-entity renderer instead of a block model (P7c).
//!
//! The 1.8 chest, ender chest, trapped chest and the two signs have no
//! blockstate model (`builtin/entity`): `TileEntityChestRenderer` and
//! `TileEntitySignRenderer` draw them. The chunk mesher therefore emits no
//! quad for them and hands the render thread a [`BlockEntityRef`] instead.
//!
//! Chests and signs exist in the reference for every block of that type, not
//! only after a server update: `Chunk.getTileEntity` creates the tile entity of
//! a block that has one on demand. A scan of the blocks is therefore the
//! faithful source for them.

use umicraft_world::{BlockState, LightLevel};

use crate::BlockSource;

/// Chest block id.
const CHEST: u16 = 54;
/// Ender chest block id.
const ENDER_CHEST: u16 = 130;
/// Trapped chest block id.
const TRAPPED_CHEST: u16 = 146;
/// Standing sign block id.
const STANDING_SIGN: u16 = 63;
/// Wall sign block id.
const WALL_SIGN: u16 = 68;

/// The tile-entity renderer family of a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEntityKind {
    /// `BlockChest` (chest).
    Chest,
    /// `BlockChest` of the trapped type.
    TrappedChest,
    /// `BlockEnderChest`.
    EnderChest,
    /// `BlockStandingSign`.
    StandingSign,
    /// `BlockWallSign`.
    WallSign,
}

impl BlockEntityKind {
    /// The kind of `state`, or `None` for a block drawn from a model.
    #[must_use]
    pub const fn of(state: BlockState) -> Option<Self> {
        match state.id() {
            CHEST => Some(Self::Chest),
            TRAPPED_CHEST => Some(Self::TrappedChest),
            ENDER_CHEST => Some(Self::EnderChest),
            STANDING_SIGN => Some(Self::StandingSign),
            WALL_SIGN => Some(Self::WallSign),
            _ => None,
        }
    }
}

/// One block that a tile-entity renderer draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockEntityRef {
    /// World position of the block.
    pub position: [i32; 3],
    /// Its wire state (the facing is in the metadata).
    pub state: BlockState,
    /// Which of the west, east, north and south neighbours hold the same
    /// block: `TileEntityChest.checkForAdjacentChests` merges two such chests
    /// into a double chest.
    pub same_neighbour: SameNeighbours,
    /// Block and sky light of the block's own cell: the renderer lights the
    /// tile entity with `getCombinedLight` of its position.
    pub light: LightLevel,
}

/// The horizontal neighbours holding the same block as a [`BlockEntityRef`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SameNeighbours {
    /// −X.
    pub west: bool,
    /// +X.
    pub east: bool,
    /// −Z.
    pub north: bool,
    /// +Z.
    pub south: bool,
}

impl BlockEntityRef {
    /// Reads the neighbours of the block at `position` from `source`.
    pub(crate) fn scan<S: BlockSource + ?Sized>(
        source: &S,
        position: [i32; 3],
        state: BlockState,
    ) -> Self {
        let [x, y, z] = position;
        let same = |dx: i32, dz: i32| source.block(x + dx, y, z + dz).id() == state.id();
        Self {
            position,
            state,
            light: source.light(x, y, z),
            same_neighbour: SameNeighbours {
                west: same(-1, 0),
                east: same(1, 0),
                north: same(0, -1),
                south: same(0, 1),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Row(Vec<([i32; 3], BlockState)>);

    impl BlockSource for Row {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.0
                .iter()
                .find(|(position, _)| *position == [x, y, z])
                .map_or(BlockState::AIR, |(_, state)| *state)
        }
    }

    #[test]
    fn only_chests_and_signs_are_tile_entity_blocks() {
        assert_eq!(
            BlockEntityKind::of(BlockState::new(54, 2)),
            Some(BlockEntityKind::Chest)
        );
        assert_eq!(
            BlockEntityKind::of(BlockState::new(68, 3)),
            Some(BlockEntityKind::WallSign)
        );
        assert_eq!(BlockEntityKind::of(BlockState::new(1, 0)), None);
        assert_eq!(BlockEntityKind::of(BlockState::AIR), None);
    }

    #[test]
    fn a_neighbour_of_the_same_block_is_found_but_not_another_chest_type() {
        let chest = BlockState::new(54, 2);
        let source = Row(vec![
            ([0, 0, 0], chest),
            ([1, 0, 0], chest),
            ([0, 0, 1], BlockState::new(146, 2)),
        ]);
        let found = BlockEntityRef::scan(&source, [0, 0, 0], chest);
        assert!(found.same_neighbour.east);
        assert!(!found.same_neighbour.south, "a trapped chest never merges");
        assert!(!found.same_neighbour.west && !found.same_neighbour.north);
    }
}
