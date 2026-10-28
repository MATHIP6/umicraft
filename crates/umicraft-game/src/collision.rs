// SPDX-License-Identifier: MIT OR Apache-2.0

//! Collision resolution of an axis-aligned box against the voxel grid.
//!
//! The solver is a port of MCP `net/minecraft/entity/Entity.java`:
//!
//! * `moveEntity` (line 588) resolves the three axes separately, in the order
//!   Y, X, Z, and clamps every axis against the collision boxes of the blocks
//!   the swept box touches.
//! * The step-up (line 711) tries to lift the box by `step_height` and move it
//!   over a ledge, and keeps the attempt only when it travels further than the
//!   plain move.
//! * The sneak guard (line 616) shortens the horizontal movement of a
//!   sneaking player until there is ground under the box, so that it cannot
//!   walk off a ledge.
//!
//! The hot path allocates nothing: the collision boxes are read from the
//! [`BlockSource`] on demand and the solver only ever holds a handful of
//! boxes on the stack.

use glam::DVec3;
use umicraft_world::BlockPos;

pub use crate::shapes::Aabb;
use crate::shapes::{shape_of, BlockSource};

/// The highest world coordinate that holds blocks in a 1.8 world.
const MAX_BLOCK_Y: i32 = 255;

/// The step the sneak guard shortens the movement by.
///
/// MCP `net/minecraft/entity/Entity.java:622`.
const SNEAK_SLIDE_STEP: f64 = 0.05;

/// How an entity is moved through the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveOptions {
    /// The highest ledge the entity can step up; `0.0` disables stepping.
    pub step_height: f64,
    /// Whether the entity is sneaking and must not walk off a ledge.
    pub sneaking: bool,
    /// Whether the entity rested on the ground before the move.
    pub on_ground: bool,
}

impl Default for MoveOptions {
    fn default() -> Self {
        Self {
            step_height: 0.0,
            sneaking: false,
            on_ground: false,
        }
    }
}

/// The outcome of a collision-resolved move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveResult {
    /// The movement that was actually applied.
    pub delta: DVec3,
    /// The X movement was blocked by a block.
    pub blocked_x: bool,
    /// The Z movement was blocked by a block.
    pub blocked_z: bool,
    /// The horizontal movement was blocked by a block.
    pub blocked_horizontally: bool,
    /// The vertical movement was blocked by a block.
    pub blocked_vertically: bool,
    /// The entity rests on solid ground after the move.
    pub on_ground: bool,
}

/// Moves a box through the voxel grid and resolves the collisions.
///
/// The three axes are resolved separately, exactly like the vanilla entity
/// mover: the vertical movement is clamped first, then the horizontal one on
/// each axis, so that sliding along a wall works.
///
/// The returned [`MoveResult::delta`] is the movement that was really
/// applied; the `blocked_*` flags and `on_ground` follow the vanilla
/// semantics of MCP `Entity.java:808-811`.
///
/// # Example
///
/// ```
/// use umicraft_game::collision::{move_entity, Aabb, MoveOptions};
/// use umicraft_world::World;
///
/// let world = World::new(0);
/// let bounding_box = Aabb::new(
///     glam::DVec3::new(-0.3, 0.0, -0.3),
///     glam::DVec3::new(0.3, 1.8, 0.3),
/// );
/// let result = move_entity(bounding_box, glam::DVec3::ZERO, &world, MoveOptions::default());
/// assert_eq!(result.delta, glam::DVec3::ZERO);
/// ```
pub fn move_entity<S: BlockSource + ?Sized>(
    original: Aabb,
    delta: DVec3,
    world: &S,
    options: MoveOptions,
) -> MoveResult {
    let mut requested = delta;

    // A sneaking player must not walk off a ledge: the horizontal movement is
    // shortened in 0.05 steps until there is ground under the box
    // (MCP `net/minecraft/entity/Entity.java:616`).
    if options.sneaking && options.on_ground {
        requested.x = sneak_shorten(original, requested.x, |x| DVec3::new(x, -1.0, 0.0), world);
        requested.z = sneak_shorten(original, requested.z, |z| DVec3::new(0.0, -1.0, z), world);
        while requested.x != 0.0
            && requested.z != 0.0
            && !any_collision(
                original.offset(DVec3::new(requested.x, -1.0, requested.z)),
                world,
            )
        {
            requested.x = sneak_step(requested.x);
            requested.z = sneak_step(requested.z);
        }
    }

    // The collision boxes of the swept box, MCP `Entity.java:686`.
    let swept = original.swept(requested);
    let range = BlockRange::of(swept);

    // Vertical axis, MCP `Entity.java:691`.
    let mut y = requested.y;
    for_each_collision_box(&range, &swept, world, |obstacle| {
        y = obstacle.calculate_y_offset(&original, y);
    });
    let mut current = original.offset(DVec3::new(0.0, y, 0.0));

    // Horizontal X axis, MCP `Entity.java:699`.
    let mut x = requested.x;
    for_each_collision_box(&range, &swept, world, |obstacle| {
        x = obstacle.calculate_x_offset(&current, x);
    });
    current = current.offset(DVec3::new(x, 0.0, 0.0));

    // Horizontal Z axis, MCP `Entity.java:705`.
    let mut z = requested.z;
    for_each_collision_box(&range, &swept, world, |obstacle| {
        z = obstacle.calculate_z_offset(&current, z);
    });
    current = current.offset(DVec3::new(0.0, 0.0, z));

    // The step-up, MCP `Entity.java:711`. It only runs when the entity was on
    // the ground (or was blocked while falling) and the horizontal movement
    // was shortened by a block.
    let was_on_ground = options.on_ground || (requested.y != y && requested.y < 0.0);
    if options.step_height > 0.0 && was_on_ground && (requested.x != x || requested.z != z) {
        if let Some((step, step_box)) = step_up(
            original,
            requested.x,
            requested.z,
            options.step_height,
            world,
        ) {
            // Keep the step-up only when it travels further than the plain
            // move (MCP `Entity.java:796`).
            let plain = x * x + z * z;
            let lifted = step.x * step.x + step.z * step.z;
            if lifted > plain {
                current = step_box;
                x = step.x;
                y = step.y;
                z = step.z;
            }
        }
    }

    MoveResult {
        delta: current.min - original.min,
        blocked_x: requested.x != x,
        blocked_z: requested.z != z,
        blocked_horizontally: requested.x != x || requested.z != z,
        blocked_vertically: requested.y != y,
        on_ground: (requested.y != y) && requested.y < 0.0,
    }
}

/// Lifts the box over a ledge.
///
/// Returns the movement deltas and the resulting box, or `None` when the
/// step-up is impossible. The two candidate paths follow MCP
/// `Entity.java:713-802`: the *high* path lifts the box by the full step
/// height and then moves it, the *low* path lifts it only as far as the free
/// space above the original box allows. The path that travels further wins.
///
/// The returned `y` is the vertical *delta* of the move (the drop onto the
/// obstacle), not the total lift; the box carries the total.
fn step_up<S: BlockSource + ?Sized>(
    original: Aabb,
    dx: f64,
    dz: f64,
    step_height: f64,
    world: &S,
) -> Option<(DVec3, Aabb)> {
    let step_mask = original.swept(DVec3::new(dx, step_height, dz));
    let step_range = BlockRange::of(step_mask);

    // High path: the box is lifted by the full step height.
    let reference = original.swept(DVec3::new(dx, 0.0, dz));
    let mut lift = step_height;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        lift = obstacle.calculate_y_offset(&reference, lift);
    });
    let mut high = original.offset(DVec3::new(0.0, lift, 0.0));
    let mut high_x = dx;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        high_x = obstacle.calculate_x_offset(&high, high_x);
    });
    high = high.offset(DVec3::new(high_x, 0.0, 0.0));
    let mut high_z = dz;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        high_z = obstacle.calculate_z_offset(&high, high_z);
    });
    high = high.offset(DVec3::new(0.0, 0.0, high_z));

    // Low path: the box is lifted only as far as the free space allows.
    let mut low_lift = step_height;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        low_lift = obstacle.calculate_y_offset(&original, low_lift);
    });
    let mut low = original.offset(DVec3::new(0.0, low_lift, 0.0));
    let mut low_x = dx;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        low_x = obstacle.calculate_x_offset(&low, low_x);
    });
    low = low.offset(DVec3::new(low_x, 0.0, 0.0));
    let mut low_z = dz;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        low_z = obstacle.calculate_z_offset(&low, low_z);
    });
    low = low.offset(DVec3::new(0.0, 0.0, low_z));

    // The path that travels further wins (MCP `Entity.java:774`).
    let high_wins = high_x * high_x + high_z * high_z > low_x * low_x + low_z * low_z;
    let (step_x, step_z, drop) = if high_wins {
        (high_x, high_z, -lift)
    } else {
        (low_x, low_z, -low_lift)
    };
    let mut stepped_box = if high_wins { high } else { low };

    // The box is dropped onto the obstacle it stepped over.
    let mut drop = drop;
    for_each_collision_box(&step_range, &step_mask, world, |obstacle| {
        drop = obstacle.calculate_y_offset(&stepped_box, drop);
    });
    stepped_box = stepped_box.offset(DVec3::new(0.0, drop, 0.0));

    Some((DVec3::new(step_x, drop, step_z), stepped_box))
}

/// Shortens one horizontal axis of a sneaking player until there is ground
/// below the box.
///
/// `offset` turns the axis value into the probe offset of the box (the value
/// on its own axis, -1 below). MCP `net/minecraft/entity/Entity.java:622`
/// (X) and `:638` (Z).
fn sneak_shorten<S: BlockSource + ?Sized>(
    original: Aabb,
    value: f64,
    offset: impl Fn(f64) -> DVec3,
    world: &S,
) -> f64 {
    let mut value = value;
    while value != 0.0 && !any_collision(original.offset(offset(value)), world) {
        value = sneak_step(value);
    }
    value
}

/// Moves a horizontal movement one sneak step towards zero.
fn sneak_step(value: f64) -> f64 {
    if (-SNEAK_SLIDE_STEP..SNEAK_SLIDE_STEP).contains(&value) {
        0.0
    } else if value > 0.0 {
        value - SNEAK_SLIDE_STEP
    } else {
        value + SNEAK_SLIDE_STEP
    }
}

/// True when any block collides with the given box.
pub(crate) fn any_collision<S: BlockSource + ?Sized>(bounding_box: Aabb, world: &S) -> bool {
    let range = BlockRange::of(bounding_box);
    let mut found = false;
    for_each_collision_box(&range, &bounding_box, world, |_| {
        found = true;
    });
    found
}

/// The inclusive range of blocks a box touches.
#[derive(Debug, Clone, Copy)]
struct BlockRange {
    min: BlockPos,
    max: BlockPos,
}

impl BlockRange {
    /// The blocks the box overlaps.
    ///
    /// MCP `net/minecraft/world/World.java:1265` computes the same range with
    /// `floor(min)` and `floor(max + 1)`; the vertical range starts one block
    /// lower (`i2 = k - 1`, line 1283) so that the block an entity rests on
    /// is part of the list.
    fn of(bounding_box: Aabb) -> Self {
        let min = BlockPos::new(
            bounding_box.min.x.floor() as i32,
            bounding_box.min.y.floor() as i32,
            bounding_box.min.z.floor() as i32,
        );
        let max = BlockPos::new(
            (bounding_box.max.x + 1.0).floor() as i32 - 1,
            (bounding_box.max.y + 1.0).floor() as i32 - 1,
            (bounding_box.max.z + 1.0).floor() as i32 - 1,
        );
        Self {
            min: BlockPos::new(min.x, (min.y - 1).max(0), min.z),
            max: BlockPos::new(max.x, max.y.min(MAX_BLOCK_Y), max.z),
        }
    }
}

/// Calls `f` for every collision box of the blocks in `range` that
/// intersects `mask`.
///
/// The boxes are in world space. Nothing is allocated: the boxes are built on
/// the stack and handed to the callback.
fn for_each_collision_box<S: BlockSource + ?Sized>(
    range: &BlockRange,
    mask: &Aabb,
    world: &S,
    mut f: impl FnMut(&Aabb),
) {
    for x in range.min.x..=range.max.x {
        for z in range.min.z..=range.max.z {
            for y in range.min.y..=range.max.y {
                let state = world.block(x, y, z);
                if state.is_air() {
                    continue;
                }
                let pos = BlockPos::new(x, y, z);
                let origin = DVec3::new(x as f64, y as f64, z as f64);
                for local in shape_of(state, world, pos).boxes() {
                    let obstacle = Aabb::new(local.min + origin, local.max + origin);
                    if obstacle.intersects(mask) {
                        f(&obstacle);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use umicraft_world::BlockState;

    /// A block source backed by a hash map, for tests.
    #[derive(Debug, Default)]
    struct TestBlocks {
        blocks: HashMap<(i32, i32, i32), BlockState>,
    }

    impl TestBlocks {
        fn with(blocks: &[(i32, i32, i32, BlockState)]) -> Self {
            let mut source = Self::default();
            for (x, y, z, state) in blocks {
                source.blocks.insert((*x, *y, *z), *state);
            }
            source
        }
    }

    impl BlockSource for TestBlocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.blocks
                .get(&(x, y, z))
                .copied()
                .unwrap_or(BlockState::AIR)
        }
    }

    fn player_box(y: f64) -> Aabb {
        Aabb::new(DVec3::new(-0.3, y, -0.3), DVec3::new(0.3, y + 1.8, 0.3))
    }

    #[test]
    fn a_falling_player_lands_on_the_ground() {
        let world = TestBlocks::with(&[(0, 0, 0, BlockState::new(1, 0))]);
        let result = move_entity(
            player_box(1.0),
            DVec3::new(0.0, -0.5, 0.0),
            &world,
            MoveOptions::default(),
        );
        assert_eq!(result.delta, DVec3::new(0.0, 0.0, 0.0));
        assert!(result.blocked_vertically);
        assert!(result.on_ground);
    }

    #[test]
    fn a_player_cannot_walk_into_a_wall() {
        let world = TestBlocks::with(&[(1, 0, 0, BlockState::new(1, 0))]);
        let result = move_entity(
            player_box(0.0),
            DVec3::new(1.0, 0.0, 0.0),
            &world,
            MoveOptions::default(),
        );
        // The player stops with its box touching the wall face at x = 1.
        assert_eq!(result.delta.x, 0.7);
        assert!(result.blocked_horizontally);
    }

    #[test]
    fn a_player_steps_up_a_half_block_ledge() {
        // A slab at x = 1 with its top at y = 0.5.
        let world = TestBlocks::with(&[(1, 0, 0, BlockState::new(44, 0))]);
        let result = move_entity(
            player_box(0.0),
            DVec3::new(1.0, 0.0, 0.0),
            &world,
            MoveOptions {
                step_height: 0.6,
                sneaking: false,
                on_ground: true,
            },
        );
        assert_eq!(result.delta.x, 1.0, "the player moved onto the slab");
        assert_eq!(result.delta.y, 0.5, "the player stands on top of the slab");
    }

    #[test]
    fn a_player_cannot_step_up_a_full_block() {
        let world = TestBlocks::with(&[(1, 0, 0, BlockState::new(1, 0))]);
        let result = move_entity(
            player_box(0.0),
            DVec3::new(1.0, 0.0, 0.0),
            &world,
            MoveOptions {
                step_height: 0.6,
                sneaking: false,
                on_ground: true,
            },
        );
        assert_eq!(result.delta.x, 0.7, "the player is stopped by the wall");
        assert_eq!(result.delta.y, 0.0);
    }

    #[test]
    fn a_sneaking_player_walks_along_z_on_flat_ground() {
        // Regression: the Z guard returned its unused X argument (0.0), so a
        // sneaking player could not move along Z at all. Found by the parity
        // harness: the vanilla trace sneaks at 0.0648 block/tick, Umicraft
        // stood still (docs/parity-harness.md).
        let floor: Vec<_> = (-2..=2)
            .flat_map(|x| (-2..=2).map(move |z| (x, 0, z, BlockState::new(1, 0))))
            .collect();
        let world = TestBlocks::with(&floor);
        let player = Aabb::new(DVec3::new(0.2, 1.0, 0.2), DVec3::new(0.8, 2.8, 0.8));
        let options = MoveOptions {
            step_height: 0.0,
            sneaking: true,
            on_ground: true,
        };
        for delta in [DVec3::new(0.0, 0.0, 0.06), DVec3::new(0.0, 0.0, -0.06)] {
            let moved = move_entity(player, delta, &world, options);
            assert!(
                (moved.delta.z - delta.z).abs() < 1e-9,
                "{delta} -> {}",
                moved.delta
            );
        }
    }

    #[test]
    fn a_sneaking_player_does_not_walk_off_a_ledge_along_z() {
        // The only ground ends at z = 0; the box is 0.6 deep from -0.8.
        let world = TestBlocks::with(&[(0, 0, -1, BlockState::new(1, 0))]);
        let player = Aabb::new(DVec3::new(0.2, 1.0, -0.8), DVec3::new(0.8, 2.8, -0.2));
        let moved = move_entity(
            player,
            DVec3::new(0.0, 0.0, 1.2),
            &world,
            MoveOptions {
                step_height: 0.0,
                sneaking: true,
                on_ground: true,
            },
        );
        assert!((moved.delta.z - 0.8).abs() < 1e-9, "got {}", moved.delta.z);
    }

    #[test]
    fn a_sneaking_player_does_not_walk_off_a_ledge() {
        // The only ground is the block at x = -1, which ends at x = 0. The
        // player stands on its middle with its box from -0.8 to -0.2.
        let world = TestBlocks::with(&[(-1, 0, 0, BlockState::new(1, 0))]);
        let player = Aabb::new(DVec3::new(-0.8, 1.0, -0.3), DVec3::new(-0.2, 2.8, 0.3));
        let sneaking = move_entity(
            player,
            DVec3::new(1.2, 0.0, 0.0),
            &world,
            MoveOptions {
                step_height: 0.0,
                sneaking: true,
                on_ground: true,
            },
        );
        // The sneak guard shortens the movement in 0.05 steps until the box
        // has ground under it again (MCP `Entity.java:622`). The player stops
        // with its box exactly at the edge of the block.
        assert!(
            (sneaking.delta.x - 0.8).abs() < 1e-9,
            "the sneak guard must stop the player at the edge, got {}",
            sneaking.delta.x
        );

        // Without the guard the player walks over the edge.
        let walking = move_entity(
            player,
            DVec3::new(1.2, 0.0, 0.0),
            &world,
            MoveOptions::default(),
        );
        assert_eq!(walking.delta.x, 1.2);
    }

    #[test]
    fn a_walking_player_walks_off_a_ledge() {
        let world = TestBlocks::with(&[(0, 0, 0, BlockState::new(1, 0))]);
        let result = move_entity(
            player_box(0.0),
            DVec3::new(0.5, 0.0, 0.0),
            &world,
            MoveOptions::default(),
        );
        assert_eq!(result.delta.x, 0.5);
    }
}
