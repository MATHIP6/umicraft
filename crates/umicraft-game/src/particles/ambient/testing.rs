// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test helpers: a sparse block map and short ways to read the requests.

use std::collections::HashMap;

use umicraft_world::BlockState;

use crate::particles::{ParticleKind, SpawnRequest};
use crate::shapes::BlockSource;

/// An empty (all air) world with a few blocks set by hand.
#[derive(Default)]
pub(super) struct World {
    blocks: HashMap<(i32, i32, i32), BlockState>,
}

impl World {
    /// Puts `state` at the coordinates.
    pub(super) fn set(&mut self, x: i32, y: i32, z: i32, state: BlockState) {
        self.blocks.insert((x, y, z), state);
    }
}

impl BlockSource for World {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.blocks
            .get(&(x, y, z))
            .copied()
            .unwrap_or(BlockState::AIR)
    }
}

/// The kinds of the requests, in order.
pub(super) fn kinds(requests: &[SpawnRequest<'_>]) -> Vec<ParticleKind> {
    requests.iter().map(|request| request.kind).collect()
}

/// How many of `kinds` are `kind`.
pub(super) fn count_kind(kinds: &[ParticleKind], kind: ParticleKind) -> usize {
    kinds.iter().filter(|&&other| other == kind).count()
}
