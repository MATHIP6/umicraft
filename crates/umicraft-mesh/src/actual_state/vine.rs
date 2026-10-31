// SPDX-License-Identifier: MIT OR Apache-2.0

//! Vines: `BlockVine.getActualState`.

use super::Overrides;
use crate::BlockSource;

/// Vine block id.
const VINE: u16 = 106;

/// Whether `id` is a vine.
pub(super) fn handles(id: u16) -> bool {
    id == VINE
}

/// The `up` flag of the vine at `position`.
///
/// The side flags come from the metadata bits and are already in the registry
/// key; only the top face depends on the world: it is drawn when the block
/// above is a normal cube (`BlockVine.getActualState`, `isBlockNormalCube`,
/// approximated by `BlockState::is_opaque`).
pub(super) fn overrides<S: BlockSource + ?Sized>(source: &S, position: [i32; 3]) -> Overrides {
    let above = source.block(position[0], position[1] + 1, position[2]);
    let mut overrides = Overrides::default();
    overrides.set_flag("up", above.is_opaque());
    overrides
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    #[test]
    fn a_vine_under_stone_shows_its_top() {
        let grid = Grid::default().with([0, 1, 0], 1, 0);
        assert_eq!(overrides(&grid, [0, 0, 0]).as_slice(), [("up", "true")]);
    }

    #[test]
    fn a_vine_under_air_or_a_vine_has_no_top() {
        let grid = Grid::default();
        assert_eq!(overrides(&grid, [0, 0, 0]).as_slice(), [("up", "false")]);
        let grid = Grid::default().with([0, 1, 0], 106, 1);
        assert_eq!(overrides(&grid, [0, 0, 0]).as_slice(), [("up", "false")]);
    }
}
