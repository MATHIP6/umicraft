// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block and chunk coordinates.

/// A block position inside the 256-block-tall 1.8 world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockPos {
    /// Block X in world coordinates.
    pub x: i32,
    /// Block Y, 0..=255 for loaded terrain.
    pub y: i32,
    /// Block Z in world coordinates.
    pub z: i32,
}

impl BlockPos {
    /// Creates a block position.
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Chunk column containing this position.
    #[must_use]
    pub const fn chunk(self) -> ChunkPos {
        ChunkPos::new(self.x >> 4, self.z >> 4)
    }

    /// Chunk-local X (0..=15).
    #[must_use]
    pub const fn local_x(self) -> u8 {
        (self.x & 15) as u8
    }

    /// Chunk-local Z (0..=15).
    #[must_use]
    pub const fn local_z(self) -> u8 {
        (self.z & 15) as u8
    }
}

/// Chunk column coordinates: X and Z of the 16×16 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkPos {
    /// Chunk X (block X >> 4).
    pub x: i32,
    /// Chunk Z (block Z >> 4).
    pub z: i32,
}

impl ChunkPos {
    /// Creates a chunk position.
    #[must_use]
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Block position of the column's minimum corner (y = 0).
    #[must_use]
    pub const fn origin(self) -> BlockPos {
        BlockPos::new(self.x << 4, 0, self.z << 4)
    }

    /// True when `pos` falls inside this chunk column.
    #[must_use]
    pub const fn contains(self, pos: BlockPos) -> bool {
        pos.x >> 4 == self.x && pos.z >> 4 == self.z
    }

    /// The four horizontal neighbours, in west/east/north/south order.
    #[must_use]
    pub const fn neighbours(self) -> [Self; 4] {
        [
            Self::new(self.x - 1, self.z),
            Self::new(self.x + 1, self.z),
            Self::new(self.x, self.z - 1),
            Self::new(self.x, self.z + 1),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_coordinates_use_floor_division_by_shifting() {
        let pos = BlockPos::new(-1, 0, -17);
        assert_eq!(pos.chunk(), ChunkPos::new(-1, -2));
        assert_eq!(pos.local_x(), 15);
        assert_eq!(pos.local_z(), 15);
    }

    #[test]
    fn chunk_neighbours_are_west_east_north_south() {
        let centre = ChunkPos::new(3, 4);
        assert_eq!(
            centre.neighbours(),
            [
                ChunkPos::new(2, 4),
                ChunkPos::new(4, 4),
                ChunkPos::new(3, 3),
                ChunkPos::new(3, 5)
            ]
        );
        assert!(centre.contains(BlockPos::new(48, 0, 64)));
        assert!(!centre.contains(BlockPos::new(64, 0, 64)));
    }
}
