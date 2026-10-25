// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block states and the coarse shape/opacity classes the M2 mesher needs.
//!
//! A 1.8 wire block state is `block id << 4 | metadata`
//! (`Block.BLOCK_STATE_IDS` in MCP), which fits in 16 bits. M5 replaces this
//! with a real block registry; M2 only needs to tell air, full opaque cubes,
//! see-through cubes, liquids, and non-cube decorations apart.

/// A 1.8 global block state: 12-bit block id plus 4-bit metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BlockState(u16);

/// Coarse rendering class of a block state.
///
/// The classes drive the naive mesher's face culling. Shapes and per-state
/// models arrive with the block registry in M5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// No geometry and does not occlude neighbours.
    Air,
    /// Full cube that hides the neighbouring face.
    Opaque,
    /// Full cube that is see-through (glass, leaves, ice) and does not hide
    /// neighbouring faces, but hides faces against an identical state.
    Transparent,
    /// Full-cube liquid (water/lava), treated like [`BlockKind::Transparent`]
    /// for culling.
    Liquid,
    /// Non-cube decoration (plants, torches, slabs, fences, …). The M2 mesher
    /// skips these; M5 adds models.
    Flat,
}

impl BlockState {
    /// Air (`block id 0`, metadata 0).
    pub const AIR: Self = Self(0);

    /// Wraps a raw wire state (`block id << 4 | metadata`).
    #[must_use]
    pub const fn from_wire(raw: u16) -> Self {
        Self(raw)
    }

    /// Creates a state from a block id and metadata; metadata is truncated to
    /// four bits.
    #[must_use]
    pub const fn new(id: u16, metadata: u8) -> Self {
        Self((id << 4) | (metadata as u16 & 0x0F))
    }

    /// Raw wire representation (`block id << 4 | metadata`).
    #[must_use]
    pub const fn wire(self) -> u16 {
        self.0
    }

    /// 12-bit block id.
    #[must_use]
    pub const fn id(self) -> u16 {
        self.0 >> 4
    }

    /// 4-bit metadata.
    #[must_use]
    pub const fn metadata(self) -> u8 {
        (self.0 & 0x0F) as u8
    }

    /// True for air.
    #[must_use]
    pub const fn is_air(self) -> bool {
        self.0 == 0
    }

    /// Coarse rendering class, see [`BlockKind`].
    #[must_use]
    pub const fn kind(self) -> BlockKind {
        match self.id() {
            0 => BlockKind::Air,
            // Water and lava are full cubes that do not cull neighbours.
            8..=11 => BlockKind::Liquid,
            // See-through full cubes.
            18 | 20 | 79 | 95 => BlockKind::Transparent,
            // Non-cube blocks (slabs, stairs, fences, doors, rails, plants,
            // torches, etc.). Unknown blocks default to opaque cubes so a
            // missing registry entry never opens holes in the terrain.
            6
            | 26..=28
            | 30..=32
            | 34
            | 36..=40
            | 44
            | 50
            | 51
            | 53
            | 55
            | 59
            | 63..=72
            | 75..=78
            | 81
            | 83
            | 85
            | 90
            | 92..=94
            | 96
            | 101
            | 102
            | 104..=109
            | 111
            | 113..=120
            | 126..=128
            | 131
            | 132
            | 134..=136
            | 139..=151
            | 154
            | 156
            | 157
            | 160
            | 163
            | 164
            | 167
            | 171
            | 175..=178
            | 180
            | 182..=205 => BlockKind::Flat,
            _ => BlockKind::Opaque,
        }
    }

    /// True when the block is a full cube that hides the face of its
    /// neighbour.
    #[must_use]
    pub const fn is_opaque(self) -> bool {
        matches!(self.kind(), BlockKind::Opaque)
    }

    /// True when the M2 mesher emits geometry for this state.
    #[must_use]
    pub const fn is_renderable(self) -> bool {
        !matches!(self.kind(), BlockKind::Air | BlockKind::Flat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_layout_matches_1_8_block_state_ids() {
        let grass = BlockState::new(2, 0);
        assert_eq!(grass.wire(), 0x20);
        assert_eq!(grass.id(), 2);
        assert_eq!(grass.metadata(), 0);
        assert_eq!(BlockState::from_wire(0x70), BlockState::new(7, 0));

        let log_axis_x = BlockState::new(17, 4);
        assert_eq!(log_axis_x.wire(), 0x114);
        assert_eq!(log_axis_x.metadata(), 4);
    }

    #[test]
    fn coarse_classes() {
        assert_eq!(BlockState::AIR.kind(), BlockKind::Air);
        assert!(!BlockState::AIR.is_renderable());
        assert!(BlockState::new(1, 0).is_opaque());
        assert!(BlockState::new(8, 0).is_renderable());
        assert!(!BlockState::new(8, 0).is_opaque());
        assert!(!BlockState::new(31, 0).is_renderable());
        assert_eq!(BlockState::new(20, 0).kind(), BlockKind::Transparent);
    }

    /// Slabs, stairs and other non-full blocks must be `Flat` so they never
    /// occlude the face of an adjacent full cube.
    #[test]
    fn non_full_blocks_are_flat() {
        // Slabs
        for id in [44, 126, 182] {
            assert_eq!(
                BlockState::new(id, 0).kind(),
                BlockKind::Flat,
                "slab id {id}"
            );
            assert!(!BlockState::new(id, 0).is_opaque(), "slab id {id}");
        }
        // Stairs
        for id in [
            53, 67, 108, 109, 114, 128, 134, 135, 136, 156, 163, 164, 180,
        ] {
            assert_eq!(
                BlockState::new(id, 0).kind(),
                BlockKind::Flat,
                "stair id {id}"
            );
            assert!(!BlockState::new(id, 0).is_opaque(), "stair id {id}");
        }
        // Fences, walls, gates
        for id in [85, 107, 139, 183, 184, 185, 186, 187] {
            assert_eq!(
                BlockState::new(id, 0).kind(),
                BlockKind::Flat,
                "fence/wall id {id}"
            );
        }
        // Doors
        for id in [64, 71, 193, 194, 195, 196, 197] {
            assert_eq!(
                BlockState::new(id, 0).kind(),
                BlockKind::Flat,
                "door id {id}"
            );
        }
    }
}
