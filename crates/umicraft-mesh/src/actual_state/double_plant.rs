// SPDX-License-Identifier: MIT OR Apache-2.0

//! Double plants: `BlockDoublePlant.getActualState`.
//!
//! The upper half stores only the `half` bit (plus a facing the model ignores),
//! so the species comes from the lower half below. The species is the
//! blockstate *document* (`sunflower`, `double_grass`, ...), not a key inside
//! it, so the family overrides the document rather than a property.

use umicraft_model::definition;
use umicraft_world::BlockState;

use super::Overrides;
use crate::BlockSource;

/// Block id of `double_plant`.
const DOUBLE_PLANT: u16 = 175;
/// Metadata bit that marks the upper half.
const UPPER_HALF: u8 = 8;

/// Whether `id` is the double plant block.
pub(super) fn handles(id: u16) -> bool {
    id == DOUBLE_PLANT
}

/// The species document of an upper half, read from the lower half below.
///
/// `None` for a lower half (its metadata is already complete) or when the
/// block below is not a lower double plant.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
) -> Option<Overrides> {
    if state.metadata() & UPPER_HALF == 0 {
        return None;
    }
    let below = source.block(position[0], position[1] - 1, position[2]);
    if below.id() != DOUBLE_PLANT || below.metadata() & UPPER_HALF != 0 {
        return None;
    }
    let document = definition(DOUBLE_PLANT)?.blockstate(below.metadata())?;
    let mut overrides = Overrides::default();
    overrides.set_document(document);
    Some(overrides)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn document(grid: &Grid, y: i32, meta: u8) -> Option<&'static str> {
        overrides(grid, [0, y, 0], BlockState::new(DOUBLE_PLANT, meta))?.document
    }

    #[test]
    fn the_upper_half_uses_the_species_of_the_lower_half() {
        for (lower, expected) in [
            (0, "sunflower"),
            (1, "syringa"),
            (2, "double_grass"),
            (3, "double_fern"),
            (4, "double_rose"),
            (5, "paeonia"),
        ] {
            let grid = Grid::default().with([0, 0, 0], DOUBLE_PLANT, lower);
            assert_eq!(document(&grid, 1, 8), Some(expected));
        }
    }

    #[test]
    fn the_facing_bits_of_the_upper_half_do_not_matter() {
        let grid = Grid::default().with([0, 0, 0], DOUBLE_PLANT, 2);
        assert_eq!(document(&grid, 1, 11), Some("double_grass"));
    }

    #[test]
    fn a_lower_half_and_an_orphan_upper_half_keep_the_registry_document() {
        let grid = Grid::default().with([0, 0, 0], DOUBLE_PLANT, 2);
        assert_eq!(document(&grid, 0, 2), None);
        assert_eq!(document(&Grid::default(), 1, 8), None);
        let stacked = Grid::default().with([0, 0, 0], DOUBLE_PLANT, 8);
        assert_eq!(document(&stacked, 1, 8), None);
    }
}
