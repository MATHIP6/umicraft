// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stairs: `BlockStairs.getActualState` (corner shapes).
//!
//! A stair turns into an outer corner when the stair in front of it faces
//! sideways, and into an inner corner when the stair behind it does. Outer
//! corners win. A neighbour only counts when it has the same half, and the
//! corner is dropped when the block on the other side is a stair identical to
//! this one (`isSameStair`), which lets straight runs of stairs stay straight.

use umicraft_world::BlockState;

use super::connect::{neighbour, Side};
use super::Overrides;
use crate::BlockSource;

/// Oak, stone, brick, stone brick, nether brick, sandstone, spruce, birch,
/// jungle, quartz, acacia, dark oak and red sandstone stairs.
const STAIRS: [u16; 13] = [
    53, 67, 108, 109, 114, 128, 134, 135, 136, 156, 163, 164, 180,
];

/// Whether `id` is a stair this module computes.
pub(super) fn handles(id: u16) -> bool {
    STAIRS.contains(&id)
}

/// A stair's orientation, decoded from its metadata.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stair {
    facing: Side,
    top: bool,
}

impl Stair {
    /// `BlockStairs.getStateFromMeta`: bits 0..1 are `5 - facing index`
    /// (east, west, south, north) and bit 2 is the upside-down half.
    fn decode(state: BlockState) -> Option<Self> {
        if !handles(state.id()) {
            return None;
        }
        let meta = state.metadata();
        let facing = match meta & 3 {
            0 => Side::East,
            1 => Side::West,
            2 => Side::South,
            _ => Side::North,
        };
        Some(Self {
            facing,
            top: meta & 4 != 0,
        })
    }
}

/// The `shape` of the stair at `position`.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
) -> Overrides {
    let mut overrides = Overrides::default();
    if let Some(stair) = Stair::decode(state) {
        overrides.set("shape", shape(source, position, stair));
    }
    overrides
}

/// Outer corner first, then inner, like `getActualState`.
fn shape<S: BlockSource + ?Sized>(source: &S, position: [i32; 3], stair: Stair) -> &'static str {
    if let Some(left) = corner(source, position, stair, true) {
        return if left { "outer_left" } else { "outer_right" };
    }
    match corner(source, position, stair, false) {
        Some(true) => "inner_left",
        Some(false) => "inner_right",
        None => "straight",
    }
}

/// `func_176307_f` (outer) / `func_176305_g` (inner): `Some(true)` when the
/// corner is a left one.
///
/// The outer corner looks at the stair in front, the inner one at the stair
/// behind. That stair must share the half and face to the left or right of
/// `stair`. The block that would continue `stair`'s run must not be identical
/// to it. The model's left/right flips for upside-down stairs.
fn corner<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    stair: Stair,
    outer: bool,
) -> Option<bool> {
    let look = if outer {
        stair.facing
    } else {
        stair.facing.opposite()
    };
    let other = Stair::decode(neighbour(source, position, look))?;
    if other.top != stair.top {
        return None;
    }
    let left = if other.facing == stair.facing.rotate_y_ccw() {
        true
    } else if other.facing == stair.facing.rotate_y() {
        false
    } else {
        return None;
    };
    let check = if outer {
        other.facing.opposite()
    } else {
        other.facing
    };
    if Stair::decode(neighbour(source, position, check)) == Some(stair) {
        return None;
    }
    Some(left != stair.top)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    // Metadata: 0 east, 1 west, 2 south, 3 north; +4 upside down.
    const EAST: u8 = 0;
    const WEST: u8 = 1;
    const SOUTH: u8 = 2;
    const NORTH: u8 = 3;

    fn shape_of(grid: &Grid, meta: u8) -> &'static str {
        let state = BlockState::new(53, meta);
        overrides(grid, [0, 0, 0], state).as_slice()[0].1
    }

    #[test]
    fn a_lone_stair_is_straight() {
        assert_eq!(shape_of(&Grid::default(), EAST), "straight");
    }

    #[test]
    fn metadata_decodes_facing_and_half() {
        let stair = Stair::decode(BlockState::new(67, 6)).unwrap();
        assert!(stair.top);
        assert_eq!(stair.facing, Side::South);
        assert!(Stair::decode(BlockState::new(1, 0)).is_none());
    }

    #[test]
    fn outer_corners_follow_the_stair_in_front() {
        // East-facing stair, north-facing stair to its east.
        let grid = Grid::default().with([1, 0, 0], 53, NORTH);
        assert_eq!(shape_of(&grid, EAST), "outer_left");
        let grid = Grid::default().with([1, 0, 0], 53, SOUTH);
        assert_eq!(shape_of(&grid, EAST), "outer_right");
        let grid = Grid::default().with([-1, 0, 0], 53, NORTH);
        assert_eq!(shape_of(&grid, WEST), "outer_right");
    }

    #[test]
    fn inner_corners_follow_the_stair_behind() {
        let grid = Grid::default().with([-1, 0, 0], 53, NORTH);
        assert_eq!(shape_of(&grid, EAST), "inner_left");
        let grid = Grid::default().with([-1, 0, 0], 53, SOUTH);
        assert_eq!(shape_of(&grid, EAST), "inner_right");
        let grid = Grid::default().with([0, 0, -1], 53, WEST);
        assert_eq!(shape_of(&grid, SOUTH), "inner_right");
    }

    #[test]
    fn upside_down_stairs_swap_left_and_right() {
        let grid = Grid::default().with([1, 0, 0], 53, NORTH | 4);
        assert_eq!(shape_of(&grid, EAST | 4), "outer_right");
        let grid = Grid::default().with([-1, 0, 0], 53, NORTH | 4);
        assert_eq!(shape_of(&grid, EAST | 4), "inner_right");
    }

    #[test]
    fn a_neighbour_of_the_other_half_is_ignored() {
        let grid = Grid::default().with([1, 0, 0], 53, NORTH | 4);
        assert_eq!(shape_of(&grid, EAST), "straight");
    }

    #[test]
    fn different_stair_kinds_still_form_corners() {
        let grid = Grid::default().with([1, 0, 0], 67, NORTH);
        assert_eq!(shape_of(&grid, EAST), "outer_left");
    }

    #[test]
    fn an_identical_stair_on_the_far_side_cancels_the_corner() {
        // The south neighbour is the same east-facing stair: the run
        // continues, so the north-facing stair in front forms no corner.
        let grid = Grid::default()
            .with([1, 0, 0], 53, NORTH)
            .with([0, 0, 1], 53, EAST);
        assert_eq!(shape_of(&grid, EAST), "straight");
        let grid = Grid::default()
            .with([-1, 0, 0], 53, NORTH)
            .with([0, 0, -1], 53, EAST);
        assert_eq!(shape_of(&grid, EAST), "straight", "inner corner too");
    }

    #[test]
    fn a_same_facing_neighbour_forms_no_corner() {
        let grid = Grid::default()
            .with([1, 0, 0], 53, EAST)
            .with([-1, 0, 0], 53, EAST);
        assert_eq!(shape_of(&grid, EAST), "straight");
    }

    #[test]
    fn opposite_facing_neighbours_form_no_corner() {
        let grid = Grid::default().with([1, 0, 0], 53, WEST);
        assert_eq!(shape_of(&grid, EAST), "straight");
    }
}
