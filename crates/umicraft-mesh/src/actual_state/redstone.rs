// SPDX-License-Identifier: MIT OR Apache-2.0

//! Redstone wire: `BlockRedstoneWire.getActualState` / `getAttachPosition`.
//!
//! Each side is `none`, `side` or `up`. The power level is part of the
//! metadata and does not appear in the 1.8 blockstate keys (the colour is a
//! tint), so only the four sides are overridden.

use umicraft_world::BlockState;

use super::connect::{neighbour, Side};
use super::Overrides;
use crate::BlockSource;

/// Redstone wire block id.
const WIRE: u16 = 55;
/// Unpowered and powered repeater.
const REPEATERS: [u16; 2] = [93, 94];
/// Blocks whose `canProvidePower()` is true, except the wire and repeaters
/// handled separately: detector rail, lever, wooden/stone/gold/iron pressure
/// plates, redstone torches (lit/unlit), stone button, tripwire hook, wooden
/// button, trapped chest, comparators, daylight sensors, redstone block.
/// MCP overrides `canProvidePower` in these classes (grep of
/// `boolean canProvidePower`); there is no per-state data, so this table
/// mirrors them by id.
const POWER_PROVIDERS: [u16; 17] = [
    28, 69, 70, 72, 75, 76, 77, 131, 143, 146, 147, 148, 149, 150, 151, 152, 178,
];

/// Whether `id` is redstone wire.
pub(super) fn handles(id: u16) -> bool {
    id == WIRE
}

/// The four attach positions of the wire at `position`.
pub(super) fn overrides<S: BlockSource + ?Sized>(source: &S, position: [i32; 3]) -> Overrides {
    let mut overrides = Overrides::default();
    for side in Side::ALL {
        overrides.set(side.name(), attach_position(source, position, side));
    }
    overrides
}

/// `BlockRedstoneWire.getAttachPosition`. Like the reference it never joins
/// two wires in a straight line into a cross: each side is decided alone.
fn attach_position<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    side: Side,
) -> &'static str {
    let [x, y, z] = position;
    let (dx, dz) = side.offset();
    let target = neighbour(source, position, side);
    let below = source.block(x + dx, y - 1, z + dz);
    if can_connect_to(target, Some(side)) || (!target.is_opaque() && can_connect_up(below)) {
        return "side";
    }
    let above = source.block(x, y + 1, z);
    let diagonal_above = source.block(x + dx, y + 1, z + dz);
    if !above.is_opaque() && target.is_opaque() && can_connect_up(diagonal_above) {
        "up"
    } else {
        "none"
    }
}

/// `canConnectUpwardsTo`: `canConnectTo` with no side, which only wire passes.
fn can_connect_up(state: BlockState) -> bool {
    can_connect_to(state, None)
}

/// `BlockRedstoneWire.canConnectTo`: wire; a repeater whose axis runs through
/// `side`; any other power-providing block when a side is given.
fn can_connect_to(state: BlockState, side: Option<Side>) -> bool {
    let id = state.id();
    if id == WIRE {
        true
    } else if REPEATERS.contains(&id) {
        side.is_some_and(|side| {
            let facing = Side::from_horizontal_index(state.metadata());
            facing == side || facing.opposite() == side
        })
    } else {
        side.is_some() && POWER_PROVIDERS.contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn sides(grid: &Grid) -> Vec<&'static str> {
        overrides(grid, [0, 0, 0])
            .as_slice()
            .iter()
            .map(|(_, value)| *value)
            .collect()
    }

    // Order of `Side::ALL`: east, north, south, west.

    #[test]
    fn a_lone_wire_connects_to_nothing() {
        assert_eq!(sides(&Grid::default()), ["none"; 4]);
    }

    #[test]
    fn a_straight_line_only_reaches_its_neighbours() {
        let grid = Grid::default().with([1, 0, 0], WIRE, 0);
        assert_eq!(sides(&grid), ["side", "none", "none", "none"]);
    }

    #[test]
    fn wire_diagonally_below_connects_through_air() {
        let grid = Grid::default().with([0, -1, 1], WIRE, 0);
        assert_eq!(sides(&grid)[2], "side");
    }

    #[test]
    fn wire_climbs_a_block_unless_the_ceiling_blocks_it() {
        let climb = Grid::default()
            .with([1, 0, 0], 1, 0)
            .with([1, 1, 0], WIRE, 0);
        assert_eq!(sides(&climb)[0], "up");
        let capped = climb.with([0, 1, 0], 1, 0);
        assert_eq!(sides(&capped)[0], "none");
    }

    #[test]
    fn repeaters_connect_along_their_axis_only() {
        // Facing east (3) or west (1) lies on the east-west axis.
        let along = Grid::default().with([1, 0, 0], 93, 3);
        assert_eq!(sides(&along)[0], "side");
        let across = Grid::default().with([1, 0, 0], 94, 0);
        assert_eq!(sides(&across)[0], "none");
    }

    #[test]
    fn power_sources_connect_and_plain_blocks_do_not() {
        let torch = Grid::default().with([0, 0, -1], 76, 0);
        assert_eq!(sides(&torch)[1], "side");
        let lever = Grid::default().with([-1, 0, 0], 69, 0);
        assert_eq!(sides(&lever)[3], "side");
        let comparator = Grid::default().with([0, 0, 1], 149, 0);
        assert_eq!(sides(&comparator)[2], "side");
        let plain = Grid::default().with([1, 0, 0], 1, 0);
        assert_eq!(sides(&plain)[0], "none");
    }
}
