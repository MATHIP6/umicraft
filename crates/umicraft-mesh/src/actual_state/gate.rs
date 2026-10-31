// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fence gates: `BlockFenceGate.getActualState`.

use super::connect::{neighbour, Side};
use super::fence::FENCE_GATES;
use super::wall::COBBLESTONE_WALL;
use super::Overrides;
use crate::BlockSource;

/// Whether `id` is a fence gate.
pub(super) fn handles(id: u16) -> bool {
    FENCE_GATES.contains(&id)
}

/// The `in_wall` flag of the gate at `position` facing `facing`: set when a
/// cobblestone wall touches either end of the gate (the sides across its
/// facing), which lowers the gate by three pixels.
///
/// `None` when `facing` is not a horizontal direction name.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    facing: &str,
) -> Option<Overrides> {
    let facing = Side::from_name(facing)?;
    let in_wall = [facing.rotate_y(), facing.rotate_y_ccw()]
        .into_iter()
        .any(|side| neighbour(source, position, side).id() == COBBLESTONE_WALL);
    let mut overrides = Overrides::default();
    overrides.set_flag("in_wall", in_wall);
    Some(overrides)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn in_wall(grid: &Grid, facing: &str) -> Option<&'static str> {
        let overrides = overrides(grid, [0, 0, 0], facing)?;
        Some(overrides.as_slice()[0].1)
    }

    #[test]
    fn a_wall_beside_the_gate_sets_in_wall() {
        let grid = Grid::default().with([1, 0, 0], 139, 0);
        assert_eq!(in_wall(&grid, "north"), Some("true"));
        assert_eq!(in_wall(&grid, "south"), Some("true"));
    }

    #[test]
    fn a_wall_in_front_of_the_gate_does_not() {
        let grid = Grid::default().with([0, 0, -1], 139, 0);
        assert_eq!(in_wall(&grid, "north"), Some("false"));
        assert_eq!(in_wall(&grid, "east"), Some("true"));
    }

    #[test]
    fn an_unknown_facing_has_no_override() {
        assert_eq!(in_wall(&Grid::default(), "up"), None);
    }
}
