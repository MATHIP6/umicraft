// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block, light and biome access for the mesher, and the off-thread chunk
//! snapshot.

use std::sync::Arc;

use umicraft_world::{BlockPos, BlockState, ChunkColumn, ChunkPos, LightLevel, World};

/// Read access to block states and light by world coordinate.
///
/// Implemented for [`World`], [`ChunkSnapshot`], and references to any
/// source. Keep implementations cheap: the mesher performs up to six block
/// lookups per block and samples light around each visible corner.
pub trait BlockSource {
    /// Block at world coordinates; unloaded or out-of-range positions must
    /// read as [`BlockState::AIR`].
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState;

    /// Light at world coordinates.
    ///
    /// Sources without light data keep the M2 full-bright look by returning
    /// [`LightLevel::FULL_SKY`]. Real world sources override this with their
    /// nibble arrays; unloaded positions use the same daylight default so the
    /// visible edge of a snapshot is not artificially dark.
    fn light(&self, _x: i32, _y: i32, _z: i32) -> LightLevel {
        LightLevel::FULL_SKY
    }

    /// Biome id at a world column, or `None` without biome data.
    ///
    /// The mesher only calls this for tinted faces (grass, leaves, vines,
    /// water). `None` keeps the face white, which is the M5 behaviour for
    /// synthetic sources and the no-assets path. Real world sources override
    /// it with the column byte; `y` does not matter.
    fn biome(&self, _x: i32, _z: i32) -> Option<u8> {
        None
    }
}

impl BlockSource for World {
    #[inline]
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        World::block(self, BlockPos::new(x, y, z))
    }

    #[inline]
    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        World::light(self, BlockPos::new(x, y, z))
    }

    #[inline]
    fn biome(&self, x: i32, z: i32) -> Option<u8> {
        World::biome(self, BlockPos::new(x, 0, z))
    }
}

impl<T: BlockSource + ?Sized> BlockSource for &T {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        (**self).block(x, y, z)
    }

    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        (**self).light(x, y, z)
    }

    fn biome(&self, x: i32, z: i32) -> Option<u8> {
        (**self).biome(x, z)
    }
}

/// A 3×3 chunk neighbourhood captured for off-thread meshing.
///
/// Chunk columns are shared behind [`Arc`], so capturing a snapshot is cheap
/// even for a full view distance.
#[derive(Debug, Clone)]
pub struct ChunkSnapshot {
    origin: ChunkPos,
    chunks: [Option<Arc<ChunkColumn>>; 9],
}

impl ChunkSnapshot {
    /// Captures the centre column and its eight neighbours from `world`.
    #[must_use]
    pub fn from_world(world: &World, center: ChunkPos) -> Self {
        let origin = ChunkPos::new(center.x - 1, center.z - 1);
        let mut chunks: [Option<Arc<ChunkColumn>>; 9] = std::array::from_fn(|_| None);
        for dz in 0..3 {
            for dx in 0..3 {
                let pos = ChunkPos::new(origin.x + dx, origin.z + dz);
                if let Some(column) = world.chunk_shared(pos) {
                    chunks[(dz * 3 + dx) as usize] = Some(column);
                }
            }
        }
        Self { origin, chunks }
    }

    /// Builds a snapshot directly from shared columns (used by tests and
    /// benches).
    #[must_use]
    pub fn from_columns(
        center: ChunkPos,
        columns: impl IntoIterator<Item = Arc<ChunkColumn>>,
    ) -> Self {
        let origin = ChunkPos::new(center.x - 1, center.z - 1);
        let mut chunks: [Option<Arc<ChunkColumn>>; 9] = std::array::from_fn(|_| None);
        for column in columns {
            let pos = column.pos();
            let dx = pos.x - origin.x;
            let dz = pos.z - origin.z;
            if (0..3).contains(&dx) && (0..3).contains(&dz) {
                chunks[(dz * 3 + dx) as usize] = Some(column);
            }
        }
        Self { origin, chunks }
    }
}

impl BlockSource for ChunkSnapshot {
    #[inline]
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        if !(0..256).contains(&y) {
            return BlockState::AIR;
        }
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        let dx = chunk.x - self.origin.x;
        let dz = chunk.z - self.origin.z;
        if !(0..3).contains(&dx) || !(0..3).contains(&dz) {
            return BlockState::AIR;
        }
        self.chunks[(dz * 3 + dx) as usize]
            .as_ref()
            .map_or(BlockState::AIR, |column| {
                column.block((x & 15) as usize, y as usize, (z & 15) as usize)
            })
    }

    #[inline]
    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        if !(0..256).contains(&y) {
            return LightLevel::DARK;
        }
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        let dx = chunk.x - self.origin.x;
        let dz = chunk.z - self.origin.z;
        if !(0..3).contains(&dx) || !(0..3).contains(&dz) {
            return LightLevel::FULL_SKY;
        }
        self.chunks[(dz * 3 + dx) as usize]
            .as_ref()
            .map_or(LightLevel::FULL_SKY, |column| {
                column.light((x & 15) as usize, y as usize, (z & 15) as usize)
            })
    }

    #[inline]
    fn biome(&self, x: i32, z: i32) -> Option<u8> {
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        let dx = chunk.x - self.origin.x;
        let dz = chunk.z - self.origin.z;
        if !(0..3).contains(&dx) || !(0..3).contains(&dz) {
            return None;
        }
        self.chunks[(dz * 3 + dx) as usize]
            .as_ref()
            .map(|column| column.biomes()[((z & 15) << 4 | (x & 15)) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reads_neighbour_chunks() {
        let mut west = ChunkColumn::new(ChunkPos::new(-1, 0));
        west.set_block(15, 0, 0, BlockState::new(1, 0));
        let mut centre = ChunkColumn::new(ChunkPos::new(0, 0));
        centre.set_block(0, 0, 0, BlockState::new(1, 0));
        let snapshot =
            ChunkSnapshot::from_columns(ChunkPos::new(0, 0), [Arc::new(west), Arc::new(centre)]);
        assert_eq!(snapshot.block(-1, 0, 0), BlockState::new(1, 0));
        assert_eq!(snapshot.block(0, 0, 0), BlockState::new(1, 0));
        assert_eq!(snapshot.block(1, 0, 0), BlockState::AIR);
        assert_eq!(snapshot.block(0, 256, 0), BlockState::AIR);
    }
}
