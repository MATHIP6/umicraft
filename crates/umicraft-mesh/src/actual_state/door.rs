// SPDX-License-Identifier: MIT OR Apache-2.0

//! Doors: `BlockDoor.getActualState`.
//!
//! A door is two blocks and each half stores only part of the state: the lower
//! half has `facing` and `open`, the upper half has `hinge` and `powered`.
//! The model needs both, so every half reads the other one, but only when that
//! block is the same door (a broken door keeps its metadata-only look).
//! `powered` has no blockstate key in 1.8 and is not read.

use umicraft_model::definition;
use umicraft_world::BlockState;

use super::{parse_key, Overrides};
use crate::BlockSource;

/// Oak, iron, spruce, birch, jungle, acacia and dark oak doors.
const DOORS: [u16; 7] = [64, 71, 193, 194, 195, 196, 197];
/// Metadata bit that marks the upper half.
const UPPER_HALF: u8 = 8;

/// Whether `id` is a door this module computes.
pub(super) fn handles(id: u16) -> bool {
    DOORS.contains(&id)
}

/// The properties of the other half of the door at `position`, or `None` when
/// the other half is missing.
///
/// The lower half takes `hinge` from the block above, the upper half takes
/// `facing` and `open` from the block below.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
) -> Option<Overrides> {
    let upper = state.metadata() & UPPER_HALF != 0;
    let (dy, names): (i32, &[&'static str]) = if upper {
        (-1, &["facing", "open"])
    } else {
        (1, &["hinge"])
    };
    let other = source.block(position[0], position[1] + dy, position[2]);
    if other.id() != state.id() || (other.metadata() & UPPER_HALF != 0) == upper {
        return None;
    }
    let rule = definition(other.id())?.state(other.metadata())?;
    let props = parse_key(rule.variant);
    let mut overrides = Overrides::default();
    for name in names {
        overrides.set(name, props.get(name)?);
    }
    Some(overrides)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn at(grid: &Grid, y: i32, id: u16, meta: u8) -> Option<Vec<(&'static str, &'static str)>> {
        overrides(grid, [0, y, 0], BlockState::new(id, meta)).map(|o| o.as_slice().to_vec())
    }

    #[test]
    fn the_lower_half_takes_its_hinge_from_the_upper_half() {
        let left = Grid::default().with([0, 1, 0], 64, 8);
        assert_eq!(at(&left, 0, 64, 0), Some(vec![("hinge", "left")]));
        let right = Grid::default().with([0, 1, 0], 64, 9);
        assert_eq!(at(&right, 0, 64, 0), Some(vec![("hinge", "right")]));
    }

    #[test]
    fn the_upper_half_takes_facing_and_open_from_the_lower_half() {
        // Metadata 5: open, facing bits 1 (see the registry for the rotation).
        let closed = Grid::default().with([0, 0, 0], 71, 0);
        assert_eq!(
            at(&closed, 1, 71, 8),
            Some(vec![("facing", "east"), ("open", "false")])
        );
        let open = Grid::default().with([0, 0, 0], 71, 5);
        let facing = definition(71).and_then(|d| d.state(5)).map(|s| s.variant);
        assert_eq!(facing, Some("facing=south,half=lower,hinge=left,open=true"));
        assert_eq!(
            at(&open, 1, 71, 9),
            Some(vec![("facing", "south"), ("open", "true")])
        );
    }

    #[test]
    fn a_half_without_its_partner_keeps_the_metadata_key() {
        let grid = Grid::default();
        assert_eq!(at(&grid, 0, 64, 0), None);
        assert_eq!(at(&grid, 1, 64, 8), None);
    }

    #[test]
    fn a_different_block_or_door_is_not_a_partner() {
        let other_door = Grid::default().with([0, 1, 0], 71, 8);
        assert_eq!(at(&other_door, 0, 64, 0), None);
        let stone = Grid::default().with([0, 0, 0], 1, 0);
        assert_eq!(at(&stone, 1, 64, 8), None);
        let two_lower = Grid::default().with([0, 1, 0], 64, 0);
        assert_eq!(at(&two_lower, 0, 64, 0), None);
    }
}
