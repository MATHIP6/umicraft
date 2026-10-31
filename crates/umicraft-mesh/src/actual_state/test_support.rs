// SPDX-License-Identifier: MIT OR Apache-2.0

//! A sparse block grid for the family tests.

use std::collections::HashMap;

use umicraft_world::BlockState;

use crate::BlockSource;

/// Blocks by position; everything else reads as air.
#[derive(Default)]
pub(super) struct Grid {
    blocks: HashMap<[i32; 3], BlockState>,
}

impl Grid {
    /// Places `id:metadata` at `position`.
    pub(super) fn with(mut self, position: [i32; 3], id: u16, metadata: u8) -> Self {
        self.blocks.insert(position, BlockState::new(id, metadata));
        self
    }
}

impl BlockSource for Grid {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.blocks
            .get(&[x, y, z])
            .copied()
            .unwrap_or(BlockState::AIR)
    }
}
