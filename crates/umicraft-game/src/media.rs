// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the blocks around the player do to its movement, beyond their
//! collision boxes: ladders and vines (ADR-0014).
//!
//! Each query reads the block at the position the reference reads it, so
//! the client and the server agree on which rules apply in a tick.

use crate::physics::Player;
use crate::shapes::BlockSource;

/// The ladder block id.
const LADDER: u16 = 65;

/// The vine block id.
const VINE: u16 = 106;

/// Whether the player is on a ladder or a vine: the block at the feet,
/// `(floor(x), floor(minY), floor(z))` (MCP `EntityLivingBase.isOnLadder`,
/// line 1126).
pub fn is_on_ladder<S: BlockSource + ?Sized>(player: &Player, world: &S) -> bool {
    let block = world.block(
        player.position.x.floor() as i32,
        player.position.y.floor() as i32,
        player.position.z.floor() as i32,
    );
    matches!(block.id(), LADDER | VINE)
}
