// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a piston may do to a block: `Block.getMobilityFlag`, the
//! tile-entity and unbreakable tables, and `BlockPistonBase.canPush`.
//!
//! The reference derives the mobility from the block's `Material`
//! (`Material.getMaterialMobility`, set by `setNoPushMobility` and
//! `setImmovableMobility`), with four classes overriding it: beds, doors and
//! pressure plates return 1, ice and rails return 0. The tables below are the
//! result of walking every `Block.registerBlock(id, ...)` call in 1.8 and
//! reading the constructor's material; unknown ids are normal blocks.

use umicraft_world::BlockState;

/// Block id of `sticky_piston`.
pub const STICKY_PISTON: u16 = 29;
/// Block id of `piston`.
pub const PISTON: u16 = 33;
/// Block id of `obsidian`.
pub const OBSIDIAN: u16 = 49;
/// Block id of `slime` (1.8 adds it; pistons pull its neighbours along).
pub const SLIME: u16 = 165;
/// The metadata bit of a piston base that says it is extended
/// (`BlockPistonBase.EXTENDED`, bit 3 in `getMetaFromState`).
const EXTENDED_BIT: u8 = 8;
/// The world is 256 blocks tall (`World.getHeight`).
const WORLD_HEIGHT: i32 = 256;

/// How a block reacts to a piston (`Block.getMobilityFlag`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mobility {
    /// Flag 0: the block is pushed.
    Normal,
    /// Flag 1: the block is not pushed, a piston breaks it instead.
    Destroy,
    /// Flag 2: the block stops the piston.
    Immovable,
}

/// Blocks with flag 1: plants, liquids, leaves, web, fire, snow layer,
/// cactus, gourds, cake, dragon egg, and the `Material.circuits` family
/// (torches, wire, levers, buttons, repeaters, comparators, ladder, tripwire,
/// skull, flower pot) plus beds, doors and pressure plates, which override
/// the flag.
const DESTROY: &[u16] = &[
    6, 8, 9, 10, 11, 18, 26, 30, 31, 32, 37, 38, 39, 40, 50, 51, 55, 59, 64, 65, 69, 70, 71, 72,
    75, 76, 77, 78, 81, 83, 86, 91, 92, 93, 94, 103, 104, 105, 106, 111, 115, 122, 127, 131, 132,
    140, 141, 142, 143, 144, 147, 148, 149, 150, 161, 175, 193, 194, 195, 196, 197,
];

/// Blocks with flag 2: `Material.piston` (both pistons, the head and the
/// moving block), `anvil`, `portal` (nether and end) and `barrier`.
const IMMOVABLE: &[u16] = &[29, 33, 34, 36, 90, 119, 145, 166];

/// Blocks of the `BlockContainer` family (they carry a tile entity), plus the
/// two comparators, which implement `ITileEntityProvider` themselves.
const TILE_ENTITY: &[u16] = &[
    23, 25, 52, 54, 61, 62, 63, 68, 84, 116, 117, 119, 130, 137, 138, 140, 144, 146, 149, 150, 151,
    154, 158, 176, 177, 178,
];

/// Blocks with hardness -1 (`setBlockUnbreakable`, or `setHardness(-1.0F)`):
/// bedrock, nether portal, end portal and its frame, command block, barrier.
const UNBREAKABLE: &[u16] = &[7, 90, 119, 120, 137, 166];

/// `Block.getMobilityFlag` of a block id; unknown ids are normal.
#[must_use]
pub fn mobility(id: u16) -> Mobility {
    if DESTROY.contains(&id) {
        Mobility::Destroy
    } else if IMMOVABLE.contains(&id) {
        Mobility::Immovable
    } else {
        Mobility::Normal
    }
}

/// Whether the block carries a tile entity (`instanceof ITileEntityProvider`),
/// which a piston never moves.
#[must_use]
pub fn has_tile_entity(id: u16) -> bool {
    TILE_ENTITY.contains(&id)
}

/// Whether the block has hardness -1.
#[must_use]
pub fn is_unbreakable(id: u16) -> bool {
    UNBREAKABLE.contains(&id)
}

/// `BlockPistonBase.canPush`: whether a piston moving along `direction` may
/// move (or, with `allow_destroy`, break) `state` at `pos`.
///
/// Deviation: the world border check is skipped, the client does not track
/// the border and the server only sends `Block Action` for legal moves.
#[must_use]
pub fn can_push(
    state: BlockState,
    pos: [i32; 3],
    direction: [i32; 3],
    allow_destroy: bool,
) -> bool {
    let id = state.id();
    if id == OBSIDIAN {
        return false;
    }
    let y = pos[1];
    let top = WORLD_HEIGHT - 1;
    if y < 0
        || y > top
        || (direction == [0, -1, 0] && y == 0)
        || (direction == [0, 1, 0] && y == top)
    {
        return false;
    }
    if id == PISTON || id == STICKY_PISTON {
        return state.metadata() & EXTENDED_BIT == 0;
    }
    if is_unbreakable(id) {
        return false;
    }
    match mobility(id) {
        Mobility::Immovable => false,
        Mobility::Destroy => allow_destroy,
        Mobility::Normal => !has_tile_entity(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: u16) -> BlockState {
        BlockState::new(id, 0)
    }

    #[test]
    fn classified_ids_match_the_reference_examples() {
        // One expectation per family; this walks 0..=197 to catch typos.
        for id in 0..=197u16 {
            let expected = match id {
                29 | 33 | 34 | 36 | 90 | 119 | 145 | 166 => Mobility::Immovable,
                6
                | 8..=11
                | 18
                | 26
                | 30..=32
                | 37..=40
                | 50
                | 51
                | 55
                | 59
                | 64
                | 65
                | 69..=72
                | 75..=78
                | 81
                | 83
                | 86
                | 91..=94
                | 103..=106
                | 111
                | 115
                | 122
                | 127
                | 131
                | 132
                | 140..=144
                | 147..=150
                | 161
                | 175
                | 193..=197 => Mobility::Destroy,
                _ => Mobility::Normal,
            };
            assert_eq!(mobility(id), expected, "mobility of block {id}");
        }
    }

    #[test]
    fn the_tables_hold_the_known_blocks() {
        assert!(is_unbreakable(7) && is_unbreakable(166) && !is_unbreakable(49));
        assert!(has_tile_entity(54) && has_tile_entity(61) && !has_tile_entity(1));
        assert_eq!(mobility(50), Mobility::Destroy); // torch
        assert_eq!(mobility(31), Mobility::Destroy); // tall grass
        assert_eq!(mobility(27), Mobility::Normal); // rails override to 0
        assert_eq!(mobility(79), Mobility::Normal); // ice overrides to 0
        assert_eq!(mobility(1), Mobility::Normal);
    }

    #[test]
    fn tables_are_sorted_free_of_duplicates() {
        for table in [DESTROY, IMMOVABLE, TILE_ENTITY, UNBREAKABLE] {
            let mut ids = table.to_vec();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), table.len());
            assert!(table.iter().all(|id| *id <= 197));
        }
    }

    #[test]
    fn obsidian_bedrock_chests_and_immovables_stay() {
        for id in [49, 7, 54, 145, 90, 166, 61] {
            assert!(!can_push(state(id), [0, 64, 0], [1, 0, 0], true), "{id}");
        }
        assert!(can_push(state(1), [0, 64, 0], [1, 0, 0], false));
    }

    #[test]
    fn destroyable_blocks_need_the_destroy_flag() {
        assert!(!can_push(state(38), [0, 64, 0], [1, 0, 0], false));
        assert!(can_push(state(38), [0, 64, 0], [1, 0, 0], true));
    }

    #[test]
    fn only_a_retracted_piston_can_move() {
        assert!(can_push(
            BlockState::new(PISTON, 1),
            [0, 64, 0],
            [1, 0, 0],
            false
        ));
        assert!(!can_push(
            BlockState::new(PISTON, 9),
            [0, 64, 0],
            [1, 0, 0],
            false
        ));
        assert!(!can_push(
            BlockState::new(STICKY_PISTON, 8),
            [0, 64, 0],
            [1, 0, 0],
            false
        ));
    }

    #[test]
    fn the_world_height_limits_vertical_pushes() {
        assert!(!can_push(state(1), [0, 0, 0], [0, -1, 0], false));
        assert!(can_push(state(1), [0, 0, 0], [0, 1, 0], false));
        assert!(!can_push(state(1), [0, 255, 0], [0, 1, 0], false));
        assert!(can_push(state(1), [0, 255, 0], [0, -1, 0], false));
        assert!(!can_push(state(1), [0, 256, 0], [1, 0, 0], false));
        assert!(!can_push(state(1), [0, -1, 0], [1, 0, 0], false));
    }
}
