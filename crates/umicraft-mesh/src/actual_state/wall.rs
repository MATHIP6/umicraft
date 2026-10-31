// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cobblestone walls: `BlockWall.getActualState` / `canConnectTo`.

use umicraft_world::BlockState;

use super::connect::{is_solid_cube, neighbour, Side};
use super::fence::FENCE_GATES;
use super::Overrides;
use crate::BlockSource;

/// Cobblestone wall (both variants share the id).
pub(super) const COBBLESTONE_WALL: u16 = 139;

/// Whether `id` is a wall this module computes.
pub(super) fn handles(id: u16) -> bool {
    id == COBBLESTONE_WALL
}

/// The `up` post flag and the four connection flags of the wall at
/// `position`.
pub(super) fn overrides<S: BlockSource + ?Sized>(source: &S, position: [i32; 3]) -> Overrides {
    let mut overrides = Overrides::default();
    // The post is drawn whenever anything sits on top (`!isAirBlock`).
    let above = source.block(position[0], position[1] + 1, position[2]);
    overrides.set_flag("up", above != BlockState::AIR);
    for side in Side::ALL {
        let other = neighbour(source, position, side);
        overrides.set_flag(side.name(), connects(other));
    }
    overrides
}

/// `BlockWall.canConnectTo`: another wall, any fence gate, or a solid cube.
fn connects(other: BlockState) -> bool {
    other.id() == COBBLESTONE_WALL || FENCE_GATES.contains(&other.id()) || is_solid_cube(other)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn flags(grid: &Grid) -> Vec<(&'static str, &'static str)> {
        overrides(grid, [0, 0, 0]).as_slice().to_vec()
    }

    #[test]
    fn a_lone_wall_has_no_post_and_no_arms() {
        assert!(flags(&Grid::default())
            .iter()
            .all(|(_, value)| *value == "false"));
    }

    #[test]
    fn any_block_above_raises_the_post() {
        let grid = Grid::default().with([0, 1, 0], 50, 5); // torch
        assert_eq!(flags(&grid)[0], ("up", "true"));
    }

    #[test]
    fn walls_join_walls_gates_and_solid_cubes() {
        let grid = Grid::default()
            .with([1, 0, 0], 139, 1)
            .with([0, 0, -1], 107, 0)
            .with([-1, 0, 0], 1, 0)
            .with([0, 0, 1], 86, 0); // pumpkin
        assert_eq!(
            flags(&grid),
            [
                ("up", "false"),
                ("east", "true"),
                ("north", "true"),
                ("south", "false"),
                ("west", "true")
            ]
        );
    }
}
