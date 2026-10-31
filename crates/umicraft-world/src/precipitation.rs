// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where rain and snow stop (`Chunk.getPrecipitationHeight`).
//!
//! The reference caches a height map per column; this port scans the column
//! on demand with the same definition, so a block change never leaves a stale
//! cache behind. The weather pass reads it for the few hundred columns around
//! the camera once per game tick.
//!
//! Source: MCP `Chunk.getPrecipitationHeight` and `World.getPrecipitationHeight`.

use crate::block::BlockState;
use crate::chunk::{ChunkColumn, SECTION_COUNT};
use crate::fluid::{lets_movement_through, Liquid};
use crate::pos::BlockPos;
use crate::section::SECTION_SIZE;
use crate::world::World;

/// `Material.blocksMovement() || Material.isLiquid()`: what catches a drop.
fn catches_precipitation(state: BlockState) -> bool {
    !lets_movement_through(state) || Liquid::of(state).is_some()
}

impl ChunkColumn {
    /// The block height just above the highest block that blocks movement or
    /// is a liquid at column-local `x`, `z`: the first cell rain reaches.
    ///
    /// Like the reference the scan starts at the top of the highest stored
    /// section (`getTopFilledSegment() + 15`) and never looks at `y = 0`, so a
    /// column with nothing to stop the rain gives `-1`. Out-of-range
    /// coordinates read as an empty column.
    ///
    /// ```
    /// use umicraft_world::{BlockState, ChunkColumn, ChunkPos};
    ///
    /// let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
    /// assert_eq!(column.precipitation_height(3, 4), -1);
    /// column.set_block(3, 63, 4, BlockState::new(2, 0));
    /// assert_eq!(column.precipitation_height(3, 4), 64);
    /// ```
    #[must_use]
    pub fn precipitation_height(&self, x: usize, z: usize) -> i32 {
        let top_segment = (0..SECTION_COUNT)
            .rev()
            .find(|&index| self.section(index).is_some())
            .map_or(0, |index| index * SECTION_SIZE);
        (1..=top_segment + SECTION_SIZE - 1)
            .rev()
            .find(|&y| catches_precipitation(self.block(x, y, z)))
            .map_or(-1, |y| y as i32 + 1)
    }
}

impl World {
    /// [`ChunkColumn::precipitation_height`] at a world column, `None` while
    /// its chunk is not loaded.
    #[must_use]
    pub fn precipitation_height(&self, x: i32, z: i32) -> Option<i32> {
        let pos = BlockPos::new(x, 0, z);
        self.chunk(pos.chunk()).map(|column| {
            column.precipitation_height(pos.local_x() as usize, pos.local_z() as usize)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkPayload;
    use crate::pos::ChunkPos;

    const GRASS: BlockState = BlockState::new(2, 0);
    const WATER: BlockState = BlockState::new(9, 0);
    const LEAVES: BlockState = BlockState::new(18, 0);
    const GLASS_PANE: BlockState = BlockState::new(102, 0);
    const TALL_GRASS: BlockState = BlockState::new(31, 1);
    const SNOW_LAYER: BlockState = BlockState::new(78, 0);

    fn column_with(y: usize, state: BlockState) -> ChunkColumn {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        column.set_block(1, y, 2, state);
        column
    }

    #[test]
    fn rain_stops_on_the_top_of_a_solid_block() {
        assert_eq!(column_with(63, GRASS).precipitation_height(1, 2), 64);
    }

    #[test]
    fn a_liquid_catches_rain() {
        assert_eq!(column_with(62, WATER).precipitation_height(1, 2), 63);
    }

    #[test]
    fn leaves_and_glass_panes_block_movement() {
        assert_eq!(column_with(90, LEAVES).precipitation_height(1, 2), 91);
        assert_eq!(column_with(70, GLASS_PANE).precipitation_height(1, 2), 71);
    }

    #[test]
    fn plants_and_snow_layers_let_the_rain_through() {
        let mut column = column_with(64, TALL_GRASS);
        column.set_block(1, 65, 2, SNOW_LAYER);
        assert_eq!(column.precipitation_height(1, 2), -1);
        column.set_block(1, 40, 2, GRASS);
        assert_eq!(column.precipitation_height(1, 2), 41);
    }

    #[test]
    fn an_empty_column_has_no_precipitation_height() {
        let column = ChunkColumn::new(ChunkPos::new(0, 0));
        assert_eq!(column.precipitation_height(0, 0), -1);
    }

    #[test]
    fn the_highest_blocking_block_wins() {
        let mut column = column_with(30, GRASS);
        column.set_block(1, 100, 2, LEAVES);
        assert_eq!(column.precipitation_height(1, 2), 101);
    }

    #[test]
    fn the_world_lookup_uses_world_coordinates_and_needs_the_chunk() {
        let mut world = World::new(0);
        assert_eq!(world.precipitation_height(-15, 17), None);
        // One all-air section: blocks, block light, sky light and biomes.
        let data = vec![0u8; 8192 + 2048 + 2048 + 256];
        let payload = ChunkPayload {
            primary_bit_mask: 1,
            ground_up: true,
            data: &data,
        };
        world.apply_chunk(ChunkPos::new(-1, 1), payload).unwrap();
        assert_eq!(world.precipitation_height(-15, 17), Some(-1));
        world.set_block(BlockPos::new(-15, 70, 17), GRASS);
        assert_eq!(world.precipitation_height(-15, 17), Some(71));
    }
}
