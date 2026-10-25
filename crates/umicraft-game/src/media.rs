// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the blocks around the player do to its movement, beyond their
//! collision boxes: ladders and vines, cobwebs and soul sand (ADR-0014).
//!
//! Each query reads the block at the position the reference reads it, so
//! the client and the server agree on which rules apply in a tick.

use glam::DVec3;

use crate::physics::Player;
use crate::shapes::BlockSource;

/// The ladder block id.
const LADDER: u16 = 65;

/// The vine block id.
const VINE: u16 = 106;

/// The cobweb block id.
const COBWEB: u16 = 30;

/// The soul sand block id.
const SOUL_SAND: u16 = 88;

/// The factor soul sand applies to the horizontal motion
/// (MCP `BlockSoulSand.java:31`).
const SOUL_SAND_FACTOR: f64 = 0.4;

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

/// The blocks the player overlaps act on it after a move.
///
/// MCP `Entity.doBlockCollisions` (line 943) calls
/// `onEntityCollidedWithBlock` for every block between the corners of the box
/// shrunk by 0.001: a cobweb slows the next move (`BlockWeb`, through
/// `Entity.setInWeb`), soul sand slows the horizontal motion
/// (`BlockSoulSand.java:29`).
pub fn collide_with_blocks<S: BlockSource + ?Sized>(player: &mut Player, world: &S) {
    let bounds = player.bounding_box();
    let low = bounds.min + DVec3::splat(0.001);
    let high = bounds.max - DVec3::splat(0.001);
    for x in low.x.floor() as i32..=high.x.floor() as i32 {
        for y in low.y.floor() as i32..=high.y.floor() as i32 {
            for z in low.z.floor() as i32..=high.z.floor() as i32 {
                match world.block(x, y, z).id() {
                    COBWEB => player.in_web = true,
                    SOUL_SAND => {
                        player.velocity.x *= SOUL_SAND_FACTOR;
                        player.velocity.z *= SOUL_SAND_FACTOR;
                    }
                    _ => {}
                }
            }
        }
    }
}
