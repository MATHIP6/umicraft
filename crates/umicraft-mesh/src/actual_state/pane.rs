// SPDX-License-Identifier: MIT OR Apache-2.0

//! Glass panes and iron bars: `BlockPane.getActualState` /
//! `canPaneConnectToBlock`.

use umicraft_world::BlockState;

use super::connect::{neighbour, Side};
use super::Overrides;
use crate::BlockSource;

/// Iron bars, glass pane and stained glass pane (every `BlockPane`).
const PANES: [u16; 3] = [101, 102, 160];
/// Glass and stained glass: full blocks that are not opaque but panes join.
const GLASS: [u16; 2] = [20, 95];

/// Whether `id` is a pane this module computes.
pub(super) fn handles(id: u16) -> bool {
    PANES.contains(&id)
}

/// The four connection flags of the pane at `position`.
pub(super) fn overrides<S: BlockSource + ?Sized>(source: &S, position: [i32; 3]) -> Overrides {
    let mut overrides = Overrides::default();
    for side in Side::ALL {
        let other = neighbour(source, position, side);
        overrides.set_flag(side.name(), connects(other));
    }
    overrides
}

/// `canPaneConnectToBlock`: an opaque cube, glass or any pane (the "same
/// block" case is a pane too).
fn connects(other: BlockState) -> bool {
    other.is_opaque() || GLASS.contains(&other.id()) || PANES.contains(&other.id())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn flags(grid: &Grid) -> Vec<(&'static str, &'static str)> {
        overrides(grid, [0, 0, 0]).as_slice().to_vec()
    }

    #[test]
    fn a_lone_pane_connects_to_nothing() {
        assert!(flags(&Grid::default())
            .iter()
            .all(|(_, value)| *value == "false"));
    }

    #[test]
    fn panes_join_solid_blocks_glass_and_other_panes() {
        let grid = Grid::default()
            .with([1, 0, 0], 1, 0) // stone to the east
            .with([0, 0, -1], 95, 3) // stained glass to the north
            .with([-1, 0, 0], 101, 0); // iron bars to the west
        assert_eq!(
            flags(&grid),
            [
                ("east", "true"),
                ("north", "true"),
                ("south", "false"),
                ("west", "true")
            ]
        );
    }

    #[test]
    fn glass_blocks_and_stained_panes_join_but_fences_do_not() {
        let grid = Grid::default()
            .with([1, 0, 0], 20, 0)
            .with([0, 0, 1], 160, 5)
            .with([-1, 0, 0], 85, 0);
        let flags = flags(&grid);
        assert_eq!(flags[0], ("east", "true"));
        assert_eq!(flags[2], ("south", "true"));
        assert_eq!(flags[3], ("west", "false"));
    }
}
