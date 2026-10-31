// SPDX-License-Identifier: MIT OR Apache-2.0

//! The push a moving block gives to the entities it runs into.
//!
//! Source: MCP `TileEntityPiston.launchWithSlimeBlock` and
//! `BlockPistonMoving.getBoundingBox`. A block that extends sweeps the part of
//! its destination cell it has not filled yet; an entity inside that box is
//! moved along the piston by the distance the block travelled (plus
//! 1/16 so it stays ahead of the face), or, for a slime block, launched.
//! The functions here are pure; the caller applies the result to the entity
//! ([`apply`] does it for the player).

use glam::DVec3;
use umicraft_world::{BlockPos, BlockState};

use super::MovingBlock;
use crate::collision::{move_entity, Aabb, MoveOptions};
use crate::physics::{Player, STEP_HEIGHT};
use crate::shapes::{shape_of, BlockSource};

/// Block id of the slime block, which launches instead of pushing.
const SLIME_BLOCK: u16 = 165;

/// Block ids drawn as a whole cell while they move: the piston base (normal
/// and sticky) and the piston head. The reference reads the stale bounds
/// fields of these blocks, which are the full cube after any collision query.
const PISTON_PARTS: [u16; 3] = [29, 33, 34];

/// What a moving block does to an entity that overlaps its box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Push {
    /// `Entity.moveEntity(delta)`: a displacement that solid blocks can stop.
    Move(DVec3),
    /// A slime block sets one axis of the entity's motion (`0`=X, `1`=Y,
    /// `2`=Z) to `value`.
    Motion {
        /// The axis whose motion is replaced.
        axis: usize,
        /// The new motion on that axis, `-1.0` or `1.0`.
        value: f64,
    },
}

/// The push one block gives to an entity in one update.
///
/// `progress` and `distance` are the two arguments of
/// `launchWithSlimeBlock(p_145863_1_, p_145863_2_)`: the animation progress
/// and the distance to move. Returns `None` when the block has no collision
/// box or the entity does not overlap it.
///
/// The extending block's box is the cell it moves into, shortened on the far
/// side by the part it has already covered (`1 - progress`); a retracting
/// block (only launched on its last tick) uses the whole cell.
#[must_use]
pub fn launch<S: BlockSource + ?Sized>(
    world: &S,
    block: &MovingBlock,
    progress: f32,
    distance: f32,
    entity_box: &Aabb,
) -> Option<Push> {
    let swept_progress = if block.extending {
        1.0 - progress
    } else {
        progress - 1.0
    };
    let bounds = swept_box(world, block, swept_progress)?;
    if !bounds.intersects(entity_box) {
        return None;
    }
    let direction = block.direction;
    if block.state.id() == SLIME_BLOCK && block.extending {
        let axis = direction.iter().position(|&component| component != 0)?;
        return Some(Push::Motion {
            axis,
            value: f64::from(direction[axis]),
        });
    }
    Some(Push::Move(
        direction
            .map(|component| f64::from(distance * component as f32))
            .into(),
    ))
}

/// Applies a push to the player, like `moveEntity` and the motion fields do.
///
/// A move is resolved against the blocks with the player's step height and
/// sneak guard; a launch only replaces one axis of the motion.
pub fn apply<S: BlockSource + ?Sized>(player: &mut Player, push: Push, world: &S) {
    match push {
        Push::Move(delta) => {
            let result = move_entity(
                player.bounding_box(),
                delta,
                world,
                MoveOptions {
                    step_height: STEP_HEIGHT,
                    sneaking: player.sneaking,
                    on_ground: player.on_ground,
                },
            );
            player.position += result.delta;
            player.on_ground = result.on_ground;
            player.collided_horizontally = result.blocked_horizontally;
        }
        Push::Motion { axis, value } => player.velocity[axis] = value,
    }
}

/// `BlockPistonMoving.getBoundingBox`: the collision box of the moving block
/// in world coordinates, with the face behind the motion pulled in by
/// `progress` along the piston direction.
fn swept_box<S: BlockSource + ?Sized>(
    world: &S,
    block: &MovingBlock,
    progress: f32,
) -> Option<Aabb> {
    let [x, y, z] = block.pos;
    let local = collision_hull(world, block.state, BlockPos::new(x, y, z))?;
    let mut min = local.min + DVec3::new(f64::from(x), f64::from(y), f64::from(z));
    let mut max = local.max + DVec3::new(f64::from(x), f64::from(y), f64::from(z));
    for axis in 0..3 {
        let shift = f64::from(block.direction[axis] as f32 * progress);
        if block.direction[axis] < 0 {
            min[axis] -= shift;
        } else {
            max[axis] -= shift;
        }
    }
    Some(Aabb { min, max })
}

/// The single box `Block.getCollisionBoundingBox` would give, relative to
/// the block: the hull of its collision shape. `None` for air, the moving
/// block itself and shapes without a box.
fn collision_hull<S: BlockSource + ?Sized>(
    world: &S,
    state: BlockState,
    pos: BlockPos,
) -> Option<Aabb> {
    if state.is_air() || state.id() == super::motion::EXTENSION {
        return None;
    }
    if PISTON_PARTS.contains(&state.id()) {
        return Some(Aabb::new(DVec3::ZERO, DVec3::ONE));
    }
    let shape = shape_of(state, world, pos);
    let mut boxes = shape.boxes().iter();
    let first = *boxes.next()?;
    // The shape is stored relative to the block, so `pos` only matters for
    // neighbour-dependent shapes (fences).
    Some(boxes.fold(first, |hull, other| Aabb {
        min: hull.min.min(other.min),
        max: hull.max.max(other.max),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piston::MovingBlock;
    use umicraft_world::{LightLevel, World};

    fn moving(id: u16, extending: bool) -> MovingBlock {
        MovingBlock {
            pos: [1, 0, 0],
            state: BlockState::new(id, 0),
            direction: [1, 0, 0],
            extending,
            show_head: false,
            progress: 0.5,
            last_progress: 0.0,
            light: LightLevel::FULL_SKY,
        }
    }

    fn player_box(x: f64) -> Aabb {
        Aabb::new(
            DVec3::new(x - 0.3, 0.0, -0.3),
            DVec3::new(x + 0.3, 1.8, 0.3),
        )
    }

    #[test]
    fn an_entity_in_the_swept_part_moves_by_the_step_and_a_sixteenth() {
        let world = World::new(0);
        let push = launch(&world, &moving(1, true), 0.5, 0.5625, &player_box(1.25));
        assert_eq!(push, Some(Push::Move(DVec3::new(0.5625, 0.0, 0.0))));
    }

    #[test]
    fn an_entity_beyond_the_swept_part_is_left_alone() {
        let world = World::new(0);
        // The box is x 1.0..1.5 at progress 0.5; this entity starts at 1.6.
        let push = launch(&world, &moving(1, true), 0.5, 0.5625, &player_box(1.95));
        assert_eq!(push, None);
    }

    #[test]
    fn a_slime_block_sets_the_motion_instead_of_moving() {
        let world = World::new(0);
        let push = launch(&world, &moving(165, true), 0.5, 0.5625, &player_box(1.25));
        assert_eq!(
            push,
            Some(Push::Motion {
                axis: 0,
                value: 1.0
            })
        );
    }

    #[test]
    fn a_slime_block_that_retracts_pushes_like_any_block() {
        let world = World::new(0);
        let push = launch(&world, &moving(165, false), 1.0, 0.25, &player_box(1.25));
        assert_eq!(push, Some(Push::Move(DVec3::new(0.25, 0.0, 0.0))));
    }

    #[test]
    fn the_moving_block_itself_has_no_box() {
        let world = World::new(0);
        let push = launch(&world, &moving(36, true), 0.5, 0.5625, &player_box(1.25));
        assert_eq!(push, None);
    }

    #[test]
    fn a_negative_direction_pulls_in_the_low_face() {
        let world = World::new(0);
        let mut block = moving(1, true);
        block.direction = [-1, 0, 0];
        // Box is x 1.5..2.0; the player at 1.75 overlaps it.
        let push = launch(&world, &block, 0.5, 0.5625, &player_box(1.75));
        assert_eq!(push, Some(Push::Move(DVec3::new(-0.5625, 0.0, 0.0))));
    }

    struct Wall;

    impl BlockSource for Wall {
        fn block(&self, x: i32, _y: i32, _z: i32) -> BlockState {
            if x == 2 {
                BlockState::new(1, 0)
            } else {
                BlockState::AIR
            }
        }
    }

    #[test]
    fn a_push_cannot_move_the_player_through_a_wall() {
        let mut player = Player::new(DVec3::new(1.5, 0.0, 0.5));
        apply(&mut player, Push::Move(DVec3::new(0.5625, 0.0, 0.0)), &Wall);
        // The wall starts at x = 2; the box is 0.3 wide on each side.
        assert!((player.position.x - 1.7).abs() < 1e-9);
    }

    #[test]
    fn a_launch_replaces_one_axis_of_the_motion() {
        let mut player = Player::new(DVec3::ZERO);
        player.velocity = DVec3::new(0.2, 0.3, 0.4);
        apply(
            &mut player,
            Push::Motion {
                axis: 1,
                value: 1.0,
            },
            &Wall,
        );
        assert_eq!(player.velocity, DVec3::new(0.2, 1.0, 0.4));
    }
}
