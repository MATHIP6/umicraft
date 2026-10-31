// SPDX-License-Identifier: MIT OR Apache-2.0

//! The four horizontal neighbours and the connection test shared by fences,
//! panes and walls.

use umicraft_world::BlockState;

use crate::BlockSource;

/// A horizontal direction, in the order of the blockstate properties.
///
/// `North` is −Z and `East` is +X, like `EnumFacing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    /// The four sides, alphabetical like the canonical variant keys.
    pub(super) const ALL: [Self; 4] = [Self::East, Self::North, Self::South, Self::West];

    /// Blockstate property name (`north`, `east`, ...).
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
        }
    }

    /// `(dx, dz)` of the neighbour in this direction.
    pub(super) const fn offset(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }

    /// The side after a quarter turn clockwise seen from above
    /// (`EnumFacing.rotateY`).
    pub(super) const fn rotate_y(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }

    /// The side after a quarter turn counter-clockwise (`rotateYCCW`).
    pub(super) const fn rotate_y_ccw(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::West => Self::South,
            Self::South => Self::East,
            Self::East => Self::North,
        }
    }

    /// Parses a `facing` property value.
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|side| side.name() == name)
    }
}

/// The block next to `position` on `side`.
pub(super) fn neighbour<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    side: Side,
) -> BlockState {
    let (dx, dz) = side.offset();
    source.block(position[0] + dx, position[1], position[2] + dz)
}

/// Block ids of `Material.gourd` (pumpkin, jack-o'-lantern, melon): opaque
/// full cubes that fences still refuse to touch.
const GOURDS: [u16; 3] = [86, 91, 103];
/// Barrier block id.
const BARRIER: u16 = 166;

/// `material.isOpaque() && isFullCube()` without the gourds: the solid blocks
/// a fence or a wall attaches to.
pub(super) fn is_solid_cube(state: BlockState) -> bool {
    state.is_opaque() && !GOURDS.contains(&state.id()) && state.id() != BARRIER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns_are_inverse() {
        for side in Side::ALL {
            assert_eq!(side.rotate_y().rotate_y_ccw(), side);
        }
        assert_eq!(Side::North.rotate_y(), Side::East);
        assert_eq!(Side::North.rotate_y_ccw(), Side::West);
    }

    #[test]
    fn north_is_minus_z_and_east_is_plus_x() {
        assert_eq!(Side::North.offset(), (0, -1));
        assert_eq!(Side::East.offset(), (1, 0));
    }

    #[test]
    fn solid_cubes_exclude_gourds_barriers_and_glass() {
        assert!(is_solid_cube(BlockState::new(1, 0)), "stone");
        assert!(!is_solid_cube(BlockState::new(86, 0)), "pumpkin");
        assert!(!is_solid_cube(BlockState::new(103, 0)), "melon");
        assert!(!is_solid_cube(BlockState::new(20, 0)), "glass");
        assert!(!is_solid_cube(BlockState::AIR));
    }
}
