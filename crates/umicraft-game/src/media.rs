// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the blocks around the player do to its movement, beyond their
//! collision boxes: water and its current, lava, ladders and vines, cobwebs
//! and soul sand (ADR-0017).
//!
//! Each query reads the block at the position the reference reads it, so
//! the client and the server agree on which rules apply in a tick.

use glam::DVec3;

use umicraft_world::fluid::{self, Liquid};
use umicraft_world::BlockPos;

use crate::collision;
use crate::physics::Player;
use crate::shapes::{Aabb, BlockSource};

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

/// How strongly the current pushes the player per tick
/// (MCP `World.handleMaterialAcceleration`, line 2125).
const CURRENT_PUSH: f64 = 0.014;

/// `Vec3.normalize`: a unit vector, or zero below a length of 1e-4.
fn normalize(vector: DVec3) -> DVec3 {
    let length = vector.length();
    if length < 1.0e-4 {
        DVec3::ZERO
    } else {
        vector / length
    }
}

/// Whether the player is in water, pushing it along the current.
///
/// MCP `Entity.handleWaterMovement` (line 1101) and
/// `World.handleMaterialAcceleration` (line 2077): the water blocks that
/// overlap the box shrunk by 0.4 vertically and 0.001 on every side count
/// when the top of the scanned range reaches their surface; their flow
/// directions are summed, normalised and applied as 0.014 of motion.
pub fn handle_water_movement<S: BlockSource + ?Sized>(player: &mut Player, world: &S) -> bool {
    let bounds = player.bounding_box();
    let min = bounds.min + DVec3::new(0.001, 0.4 + 0.001, 0.001);
    let max = bounds.max - DVec3::new(0.001, 0.4 + 0.001, 0.001);
    let top = (max.y + 1.0).floor() as i32;
    let mut in_water = false;
    let mut current = DVec3::ZERO;
    for x in min.x.floor() as i32..(max.x + 1.0).floor() as i32 {
        for y in min.y.floor() as i32..top {
            for z in min.z.floor() as i32..(max.z + 1.0).floor() as i32 {
                let state = world.block(x, y, z);
                if !Liquid::Water.contains(state) {
                    continue;
                }
                // The reference compares the scan bound, not the box top.
                let surface = (y + 1) as f32 - fluid::height_percent(state.metadata());
                if f64::from(top) >= f64::from(surface) {
                    in_water = true;
                    let at = |pos: BlockPos| world.block(pos.x, pos.y, pos.z);
                    current += DVec3::from(fluid::flow_vector(
                        Liquid::Water,
                        BlockPos::new(x, y, z),
                        at,
                    ));
                }
            }
        }
    }
    if current.length() > 0.0 {
        player.velocity += normalize(current) * CURRENT_PUSH;
    }
    in_water
}

/// Whether any lava overlaps the box shrunk by 0.1 horizontally and 0.4
/// vertically (MCP `Entity.isInLava`, line 1206, through
/// `World.isMaterialInBB`).
pub fn is_in_lava<S: BlockSource + ?Sized>(player: &Player, world: &S) -> bool {
    let bounds = player.bounding_box();
    let min = bounds.min + DVec3::new(0.1, 0.4, 0.1);
    let max = bounds.max - DVec3::new(0.1, 0.4, 0.1);
    for x in min.x.floor() as i32..(max.x + 1.0).floor() as i32 {
        for y in min.y.floor() as i32..(max.y + 1.0).floor() as i32 {
            for z in min.z.floor() as i32..(max.z + 1.0).floor() as i32 {
                if Liquid::Lava.contains(world.block(x, y, z)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether the player's box moved by `offset` neither collides with a block
/// nor touches a liquid (MCP `Entity.isOffsetPositionInLiquid`, line 571,
/// whose name says the opposite of what it checks).
pub fn is_free_of_blocks_and_liquid<S: BlockSource + ?Sized>(
    player: &Player,
    offset: DVec3,
    world: &S,
) -> bool {
    let bounds = player.bounding_box().offset(offset);
    !collision::any_collision(bounds, world) && !touches_liquid(bounds, world)
}

/// `World.isAnyLiquid` (line 2012): any water or lava between the floored
/// corners of the box, both included.
fn touches_liquid<S: BlockSource + ?Sized>(bounds: Aabb, world: &S) -> bool {
    for x in bounds.min.x.floor() as i32..=bounds.max.x.floor() as i32 {
        for y in bounds.min.y.floor() as i32..=bounds.max.y.floor() as i32 {
            for z in bounds.min.z.floor() as i32..=bounds.max.z.floor() as i32 {
                if Liquid::of(world.block(x, y, z)).is_some() {
                    return true;
                }
            }
        }
    }
    false
}
