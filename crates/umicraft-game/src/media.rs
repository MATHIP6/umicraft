// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the blocks around the player do to its movement, beyond their
//! collision boxes: water and its current, lava, ladders and vines, cobwebs
//! and soul sand (ADR-0014).
//!
//! Each query reads the block at the position the reference reads it, so
//! the client and the server agree on which rules apply in a tick.

use glam::DVec3;

use umicraft_world::BlockState;

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

/// True for still and flowing water (material `water`).
fn is_water(state: BlockState) -> bool {
    matches!(state.id(), 8 | 9)
}

/// True for still and flowing lava (material `lava`).
fn is_lava(state: BlockState) -> bool {
    matches!(state.id(), 10 | 11)
}

/// True when the block's material does not block movement: air, liquids,
/// plants, vines, fire, circuits, carpets, snow layers, portals and cobwebs.
///
/// Derived from the materials of `Block.registerBlocks` and
/// `Material.blocksMovement` (`MaterialLogic`, `MaterialTransparent`,
/// `MaterialLiquid`, `MaterialPortal` and the anonymous `web` material).
fn lets_movement_through(state: BlockState) -> bool {
    matches!(
        state.id(),
        0 | 6
            | 8..=11
            | 27
            | 28
            | 30..=32
            | 37..=40
            | 50
            | 51
            | 55
            | 59
            | 65
            | 66
            | 69
            | 75..=78
            | 83
            | 90
            | 93
            | 94
            | 104..=106
            | 111
            | 115
            | 119
            | 127
            | 131
            | 132
            | 140..=144
            | 149
            | 150
            | 157
            | 171
            | 175
    )
}

/// `BlockLiquid.isBlockSolid` towards a horizontal side: never the liquid
/// itself nor ice, otherwise `Material.isSolid`, which only differs from
/// [`lets_movement_through`] for the cobweb.
fn is_solid_for_water(state: BlockState) -> bool {
    !is_water(state) && state.id() != 79 && (state.id() == 30 || !lets_movement_through(state))
}

/// `BlockLiquid.getLiquidHeightPercent`: the empty share of a liquid block
/// at a level (0 is a source, 1-7 flow, 8 and above fall).
fn liquid_height_percent(level: u8) -> f32 {
    let level = if level >= 8 { 0 } else { level };
    f32::from(level + 1) / 9.0
}

/// `Vec3.normalize`: a unit vector, or zero below a length of 1e-4.
fn normalize(vector: DVec3) -> DVec3 {
    let length = vector.length();
    if length < 1.0e-4 {
        DVec3::ZERO
    } else {
        vector / length
    }
}

/// The horizontal neighbours in the order of `EnumFacing.Plane.HORIZONTAL`:
/// north, east, south, west.
const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// `BlockLiquid.getEffectiveFlowDecay` for water: the level, 0 for a falling
/// block, -1 when the block is not water.
fn flow_decay<S: BlockSource + ?Sized>(world: &S, x: i32, y: i32, z: i32) -> i32 {
    let state = world.block(x, y, z);
    if !is_water(state) {
        return -1;
    }
    let level = i32::from(state.metadata());
    if level >= 8 {
        0
    } else {
        level
    }
}

/// The direction water flows in at a block, `BlockLiquid.getFlowVector`
/// (line 150): towards the neighbours with a higher level, towards a lower
/// neighbour across an opening, and straight down for falling water next to
/// a solid side.
fn flow_vector<S: BlockSource + ?Sized>(world: &S, x: i32, y: i32, z: i32) -> DVec3 {
    let decay = flow_decay(world, x, y, z);
    let mut flow = DVec3::ZERO;
    for (dx, dz) in HORIZONTAL {
        let (nx, nz) = (x + dx, z + dz);
        let neighbour = flow_decay(world, nx, y, nz);
        let weight = if neighbour >= 0 {
            neighbour - decay
        } else if lets_movement_through(world.block(nx, y, nz)) {
            let below = flow_decay(world, nx, y - 1, nz);
            if below < 0 {
                continue;
            }
            below - (decay - 8)
        } else {
            continue;
        };
        flow += DVec3::new(f64::from(dx * weight), 0.0, f64::from(dz * weight));
    }
    if world.block(x, y, z).metadata() >= 8 {
        let walled = HORIZONTAL.iter().any(|(dx, dz)| {
            is_solid_for_water(world.block(x + dx, y, z + dz))
                || is_solid_for_water(world.block(x + dx, y + 1, z + dz))
        });
        if walled {
            flow = normalize(flow) + DVec3::new(0.0, -6.0, 0.0);
        }
    }
    normalize(flow)
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
                if !is_water(state) {
                    continue;
                }
                // The reference compares the scan bound, not the box top.
                let surface = (y + 1) as f32 - liquid_height_percent(state.metadata());
                if f64::from(top) >= f64::from(surface) {
                    in_water = true;
                    current += flow_vector(world, x, y, z);
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
                if is_lava(world.block(x, y, z)) {
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
                let state = world.block(x, y, z);
                if is_water(state) || is_lava(state) {
                    return true;
                }
            }
        }
    }
    false
}
