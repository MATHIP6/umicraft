// SPDX-License-Identifier: MIT OR Apache-2.0

//! Snowy grass, mycelium and podzol: `getActualState` of `BlockGrass`,
//! `BlockMycelium` and `BlockDirt`.
//!
//! `snowy` is true when the block above is snow (80) or a snow layer (78). The
//! metadata of these blocks never carries it, so the override always replaces
//! the key's `snowy` (the registry's metadata-bit-0 reading is only a
//! placeholder for callers without neighbours).

use umicraft_world::BlockState;

use super::Overrides;
use crate::BlockSource;

/// Snow layer block id.
const SNOW_LAYER: u16 = 78;
/// Snow block id.
const SNOW_BLOCK: u16 = 80;
/// Dirt block id; only the podzol variant has a `snowy` property.
const DIRT: u16 = 3;
/// Dirt metadata of podzol.
const PODZOL: u8 = 2;

/// Whether `id` is grass, dirt or mycelium (dirt is narrowed in
/// [`overrides`], which sees the metadata).
pub(super) fn handles(id: u16) -> bool {
    matches!(id, 2 | DIRT | 110)
}

/// The `snowy` flag of `state` at `position`; `None` for dirt that is not
/// podzol.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
) -> Option<Overrides> {
    if state.id() == DIRT && state.metadata() != PODZOL {
        return None;
    }
    let above = source.block(position[0], position[1] + 1, position[2]).id();
    let mut overrides = Overrides::default();
    overrides.set_flag("snowy", matches!(above, SNOW_LAYER | SNOW_BLOCK));
    Some(overrides)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn snowy(grid: &Grid, id: u16, meta: u8) -> Option<(&'static str, &'static str)> {
        let o = overrides(grid, [0, 0, 0], BlockState::new(id, meta))?;
        o.as_slice().first().copied()
    }

    #[test]
    fn snow_and_snow_layers_above_make_the_block_snowy() {
        for above in [78, 80] {
            let grid = Grid::default().with([0, 1, 0], above, 0);
            for (id, meta) in [(2, 0), (110, 0), (3, 2)] {
                assert_eq!(snowy(&grid, id, meta), Some(("snowy", "true")));
            }
        }
    }

    #[test]
    fn other_blocks_above_leave_it_snowy_false() {
        let air = Grid::default();
        let stone = Grid::default().with([0, 1, 0], 1, 0);
        for grid in [&air, &stone] {
            assert_eq!(snowy(grid, 2, 0), Some(("snowy", "false")));
        }
    }

    #[test]
    fn metadata_never_selects_snowy() {
        // Regression: the M5 registry reads metadata bit 0 as `snowy`.
        let grid = Grid::default();
        assert_eq!(snowy(&grid, 2, 1), Some(("snowy", "false")));
        assert_eq!(snowy(&grid, 110, 1), Some(("snowy", "false")));
    }

    #[test]
    fn plain_and_coarse_dirt_have_no_snowy_property() {
        let grid = Grid::default().with([0, 1, 0], 80, 0);
        assert_eq!(snowy(&grid, 3, 0), None);
        assert_eq!(snowy(&grid, 3, 1), None);
    }
}
