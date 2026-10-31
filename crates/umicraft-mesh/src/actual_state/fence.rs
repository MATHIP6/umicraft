// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fences: `BlockFence.getActualState` / `canConnectTo`.

use umicraft_world::BlockState;

use super::connect::{is_solid_cube, neighbour, Side};
use super::Overrides;
use crate::BlockSource;

/// Nether brick fence (`Material.rock`, so it never joins a wooden fence).
const NETHER_BRICK_FENCE: u16 = 113;
/// Oak fence and the five other wooden fences (`BlockFence` + `Material.wood`).
const WOOD_FENCES: [u16; 6] = [85, 188, 189, 190, 191, 192];
/// Oak fence gate and the five other wooden gates.
pub(super) const FENCE_GATES: [u16; 6] = [107, 183, 184, 185, 186, 187];

/// Whether `id` is a fence this module computes.
pub(super) fn handles(id: u16) -> bool {
    id == NETHER_BRICK_FENCE || WOOD_FENCES.contains(&id)
}

/// The four connection flags of the fence `id` at `position`.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    id: u16,
) -> Overrides {
    let mut overrides = Overrides::default();
    for side in Side::ALL {
        let other = neighbour(source, position, side);
        overrides.set_flag(side.name(), connects(id, other));
    }
    overrides
}

/// `BlockFence.canConnectTo`: a fence of the same material, any fence gate, or
/// a solid cube.
fn connects(id: u16, other: BlockState) -> bool {
    let same_material = if id == NETHER_BRICK_FENCE {
        other.id() == NETHER_BRICK_FENCE
    } else {
        WOOD_FENCES.contains(&other.id())
    };
    same_material || FENCE_GATES.contains(&other.id()) || is_solid_cube(other)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn flags(grid: &Grid, id: u16) -> Vec<(&'static str, &'static str)> {
        overrides(grid, [0, 0, 0], id).as_slice().to_vec()
    }

    #[test]
    fn a_lone_fence_connects_to_nothing() {
        let grid = Grid::default();
        assert!(flags(&grid, 85).iter().all(|(_, value)| *value == "false"));
    }

    #[test]
    fn fences_join_solid_blocks_gates_and_their_own_material() {
        let grid = Grid::default()
            .with([1, 0, 0], 1, 0) // stone to the east
            .with([0, 0, -1], 85, 0) // fence to the north
            .with([-1, 0, 0], 107, 0); // gate to the west
        assert_eq!(
            flags(&grid, 85),
            [
                ("east", "true"),
                ("north", "true"),
                ("south", "false"),
                ("west", "true")
            ]
        );
    }

    #[test]
    fn wood_and_nether_brick_fences_do_not_join() {
        let grid = Grid::default().with([1, 0, 0], 113, 0);
        assert_eq!(flags(&grid, 85)[0], ("east", "false"));
        let grid = Grid::default().with([1, 0, 0], 85, 0);
        assert_eq!(flags(&grid, 113)[0], ("east", "false"));
        let grid = Grid::default().with([1, 0, 0], 113, 0);
        assert_eq!(flags(&grid, 113)[0], ("east", "true"));
    }

    #[test]
    fn pumpkins_and_glass_are_not_attached() {
        let grid = Grid::default()
            .with([1, 0, 0], 86, 0)
            .with([0, 0, 1], 20, 0);
        let flags = flags(&grid, 85);
        assert_eq!(flags[0], ("east", "false"));
        assert_eq!(flags[2], ("south", "false"));
    }
}
