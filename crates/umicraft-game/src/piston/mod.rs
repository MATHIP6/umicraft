// SPDX-License-Identifier: MIT OR Apache-2.0

//! Moving pistons (P7d): the blocks a piston pushes or pulls, their
//! animation, and the push they give to a player standing against them.
//!
//! The reference does this with `BlockPistonBase.onBlockEventReceived`
//! (driven by `Block Action`), `BlockPistonStructureHelper` (which blocks
//! move), `BlockPistonMoving` and `TileEntityPiston` (the animation, 0.5 of a
//! block per tick) and `TileEntityPistonRenderer` (the drawing). The server
//! sends `Block Action` before the block changes of the same tick, so the
//! client sees the world as it was before the move, which is what
//! `BlockPistonStructureHelper` needs.
//!
//! - [`MovingBlock`] and [`MovingBlocks`]: the animation state (this file).
//! - `structure`: which blocks a piston moves or breaks.
//! - `motion`: starting the motion from a `Block Action`, and the tick.
//! - `push`: moving the player that the pushed blocks run into.

/// A block in flight, one `TileEntityPiston`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovingBlock {
    /// The cell the block is moving into (where the `piston_extension` block
    /// stands).
    pub pos: [i32; 3],
    /// The block that moves: a stone, the piston head, or the piston base.
    pub state: umicraft_world::BlockState,
    /// The direction the piston faces, as a unit vector.
    pub direction: [i32; 3],
    /// Whether the piston is extending (the block is pushed) or retracting.
    pub extending: bool,
    /// Whether the head is drawn with the block (a retracting piston base
    /// shows its head while it slides back).
    pub show_head: bool,
    /// Progress at the newest tick, `0.0..=1.0`.
    pub progress: f32,
    /// Progress at the previous tick.
    pub last_progress: f32,
    /// Light of the cell, captured when the motion starts.
    pub light: umicraft_world::LightLevel,
}

impl MovingBlock {
    /// Progress interpolated between two ticks (`TileEntityPiston.getProgress`).
    #[must_use]
    pub fn progress_at(&self, partial_tick: f32) -> f32 {
        let partial = partial_tick.min(1.0);
        self.last_progress + (self.progress - self.last_progress) * partial
    }

    /// Where the block is, relative to its cell (`getOffsetX/Y/Z`): it starts
    /// one block behind when extending and ends one block ahead of the cell
    /// when retracting.
    #[must_use]
    pub fn offset_at(&self, partial_tick: f32) -> [f32; 3] {
        let progress = self.progress_at(partial_tick);
        let distance = if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        };
        self.direction.map(|axis| axis as f32 * distance)
    }
}

/// Every block in flight.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovingBlocks {
    blocks: Vec<MovingBlock>,
}

impl MovingBlocks {
    /// The blocks in flight.
    #[must_use]
    pub fn blocks(&self) -> &[MovingBlock] {
        &self.blocks
    }

    /// Whether nothing moves.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Adds a block in flight.
    pub fn push(&mut self, block: MovingBlock) {
        self.blocks.push(block);
    }

    /// Forgets every block (a new world, a dimension change).
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(extending: bool, direction: [i32; 3], last: f32, now: f32) -> MovingBlock {
        MovingBlock {
            pos: [0, 0, 0],
            state: umicraft_world::BlockState::new(1, 0),
            direction,
            extending,
            show_head: false,
            progress: now,
            last_progress: last,
            light: umicraft_world::LightLevel::FULL_SKY,
        }
    }

    #[test]
    fn an_extending_block_arrives_from_behind() {
        let moving = block(true, [0, 1, 0], 0.0, 0.5);
        assert_eq!(moving.offset_at(0.0), [0.0, -1.0, 0.0]);
        assert_eq!(moving.offset_at(1.0), [0.0, -0.5, 0.0]);
    }

    #[test]
    fn a_retracting_block_leaves_ahead() {
        let moving = block(false, [1, 0, 0], 0.5, 1.0);
        assert_eq!(moving.offset_at(0.0), [0.5, 0.0, 0.0]);
        assert_eq!(moving.offset_at(1.0), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_partial_tick_is_capped_at_one() {
        let moving = block(true, [0, 0, 1], 0.0, 0.5);
        assert_eq!(moving.progress_at(3.0), moving.progress_at(1.0));
    }
}
