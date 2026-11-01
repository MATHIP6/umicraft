// SPDX-License-Identifier: MIT OR Apache-2.0

//! The horizontal directions of `EnumFacing`, by the index the metadata of the
//! torches, furnaces and repeaters decode to.

/// `EnumFacing.getFront` order (down, up, north, south, west, east) as the
/// `(getFrontOffsetX, getFrontOffsetZ)` of each.
const FRONT_OFFSETS: [(i32, i32); 6] = [(0, 0), (0, 0), (0, -1), (0, 1), (-1, 0), (1, 0)];

/// The `EnumFacing` index of up.
const UP: u8 = 1;
/// The `EnumFacing` index of north.
const NORTH: u8 = 2;

/// `getFrontOffsetX/Z` of the facing with that `EnumFacing` index.
pub(super) fn front_offset(facing: u8) -> (i32, i32) {
    FRONT_OFFSETS[usize::from(facing)]
}

/// The facing of a torch from its metadata (MCP `BlockTorch.getStateFromMeta`:
/// 1 east, 2 west, 3 south, 4 north, anything else up).
pub(super) fn torch_facing(metadata: u8) -> u8 {
    match metadata {
        1 => 5,
        2 => 4,
        3 => 3,
        4 => NORTH,
        _ => UP,
    }
}

/// The facing of a furnace from its metadata (MCP `BlockFurnace.getStateFromMeta`:
/// `EnumFacing.getFront`, and a vertical one reads as north).
pub(super) fn furnace_facing(metadata: u8) -> u8 {
    match metadata % 6 {
        0 | 1 => NORTH,
        facing => facing,
    }
}

/// The facing of a repeater from its metadata (MCP
/// `BlockRedstoneRepeater.getStateFromMeta`: `EnumFacing.getHorizontal`, which
/// runs south, west, north, east).
pub(super) fn repeater_facing(metadata: u8) -> u8 {
    [3, 4, NORTH, 5][usize::from(metadata & 3)]
}
