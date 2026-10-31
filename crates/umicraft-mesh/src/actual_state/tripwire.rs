// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tripwire and tripwire hooks: `BlockTripWire.getActualState` /
//! `isConnectedTo` and `BlockTripWireHook.getActualState`.

use umicraft_world::BlockState;

use super::connect::{neighbour, Side};
use super::Overrides;
use crate::BlockSource;

/// Tripwire hook block id.
const HOOK: u16 = 131;
/// Tripwire block id.
const TRIPWIRE: u16 = 132;
/// Metadata bit of a tripwire that hangs over a gap (`SUSPENDED`).
const SUSPENDED_BIT: u8 = 2;

/// Whether `id` is a tripwire or a tripwire hook.
pub(super) fn handles(id: u16) -> bool {
    id == HOOK || id == TRIPWIRE
}

/// The neighbour-dependent properties of `state` at `position`.
pub(super) fn overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
) -> Overrides {
    if state.id() == HOOK {
        hook_overrides(source, position)
    } else {
        wire_overrides(source, position, state)
    }
}

/// `BlockTripWireHook.getActualState`: `suspended` is "nothing solid below".
/// `World.doesBlockHaveSolidTopSurface` is approximated by an opaque cube
/// (it also accepts upside-down stairs and top slabs).
fn hook_overrides<S: BlockSource + ?Sized>(source: &S, position: [i32; 3]) -> Overrides {
    let below = source.block(position[0], position[1] - 1, position[2]);
    let mut overrides = Overrides::default();
    overrides.set_flag("suspended", !below.is_opaque());
    overrides
}

/// `BlockTripWire.getActualState`: one flag per side. `attached` and
/// `suspended` come from the metadata and stay in the registry key.
fn wire_overrides<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
) -> Overrides {
    let mut overrides = Overrides::default();
    for side in Side::ALL {
        overrides.set_flag(side.name(), is_connected(source, position, state, side));
    }
    overrides
}

/// `BlockTripWire.isConnectedTo`: a hook facing the wire, or another tripwire
/// with the same suspension.
fn is_connected<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
    side: Side,
) -> bool {
    let other = neighbour(source, position, side);
    match other.id() {
        HOOK => Side::from_horizontal_index(other.metadata()) == side.opposite(),
        TRIPWIRE => is_suspended(state) == is_suspended(other),
        _ => false,
    }
}

fn is_suspended(state: BlockState) -> bool {
    state.metadata() & SUSPENDED_BIT != 0
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Grid;
    use super::*;

    fn wire(grid: &Grid, metadata: u8) -> Vec<(&'static str, &'static str)> {
        overrides(grid, [0, 0, 0], BlockState::new(TRIPWIRE, metadata))
            .as_slice()
            .to_vec()
    }

    #[test]
    fn a_hook_connects_only_when_it_faces_the_wire() {
        // Hook east of the wire: facing west (1) looks at the wire.
        let facing = Grid::default().with([1, 0, 0], HOOK, 1);
        assert_eq!(wire(&facing, 0)[0], ("east", "true"));
        let away = Grid::default().with([1, 0, 0], HOOK, 3);
        assert_eq!(wire(&away, 0)[0], ("east", "false"));
    }

    #[test]
    fn tripwires_join_when_both_hang_or_both_rest() {
        let grid = Grid::default()
            .with([0, 0, -1], TRIPWIRE, 4) // north, resting
            .with([0, 0, 1], TRIPWIRE, 2); // south, suspended
        let flags = wire(&grid, 0);
        assert_eq!(flags[1], ("north", "true"));
        assert_eq!(flags[2], ("south", "false"));
        assert_eq!(wire(&grid, 2)[2], ("south", "true"));
    }

    #[test]
    fn other_blocks_do_not_connect() {
        let grid = Grid::default().with([-1, 0, 0], 1, 0);
        assert!(wire(&grid, 0).iter().all(|(_, v)| *v == "false"));
    }

    #[test]
    fn a_hook_is_suspended_without_a_solid_block_below() {
        let hook = BlockState::new(HOOK, 0);
        let open = Grid::default();
        assert_eq!(
            overrides(&open, [0, 0, 0], hook).as_slice(),
            [("suspended", "true")]
        );
        let floor = Grid::default().with([0, -1, 0], 1, 0);
        assert_eq!(
            overrides(&floor, [0, 0, 0], hook).as_slice(),
            [("suspended", "false")]
        );
    }
}
