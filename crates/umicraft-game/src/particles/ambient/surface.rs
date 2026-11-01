// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block questions the display ticks ask of their neighbours.

use umicraft_world::fluid::{lets_movement_through, Liquid};
use umicraft_world::BlockState;

/// The blocks `BlockFire.setFireInfo` gives an encouragement above 0 (MCP
/// `BlockFire.java:74-108`, ids of `Block.registerBlocks`). Dark oak and acacia
/// stairs are absent from the reference list, and so from this one.
pub(super) fn can_catch_fire(state: BlockState) -> bool {
    matches!(
        state.id(),
        5 | 17
            | 18
            | 31
            | 32
            | 35
            | 37
            | 38
            | 46
            | 47
            | 53
            | 85
            | 106
            | 107
            | 125
            | 126
            | 134..=136
            | 161
            | 162
            | 170
            | 171
            | 173
            | 175
            | 183..=192
    )
}

/// `World.doesBlockHaveSolidTopSurface` (MCP `World.java:2419`): an opaque full
/// cube, an upside-down stair or slab, a hopper, or a snow layer of height 7.
pub(super) fn has_solid_top(state: BlockState) -> bool {
    let metadata = state.metadata();
    match state.id() {
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => metadata & 4 != 0,
        44 | 126 | 182 => metadata & 8 != 0,
        154 => true,
        78 => metadata == 6,
        _ => state.is_opaque(),
    }
}

/// `!material.blocksMovement() && !material.isLiquid()`: the gap a drip needs
/// below the block it hangs from.
pub(super) fn is_open_air(state: BlockState) -> bool {
    lets_movement_through(state) && Liquid::of(state).is_none()
}
