// SPDX-License-Identifier: MIT OR Apache-2.0

//! Voxel ray casting for block targeting (GAM-3).
//!
//! The ray walks the voxel grid with an amanatides-woo digital differential
//! analyser and tests every cell it enters against the block's selection
//! shape (see [`crate::shapes::selection_shape`]). The first block whose
//! shape the ray enters is the target, and the face tells the client where
//! the new block would go.
//!
//! The reach distances are the ones of the vanilla player controller
//! (MCP `net/minecraft/client/multiplayer/PlayerControllerMP.java:340-343`).

use glam::DVec3;
use umicraft_world::BlockPos;

use crate::shapes::{selection_shape, Aabb, BlockSource};

/// The reach distance of a player in survival mode
/// (MCP `net/minecraft/client/multiplayer/PlayerControllerMP.java:342`).
pub const SURVIVAL_REACH: f64 = 4.5;

/// The reach distance of a player in creative mode
/// (MCP `net/minecraft/client/multiplayer/PlayerControllerMP.java:342`).
pub const CREATIVE_REACH: f64 = 5.0;

/// A hard limit on the number of cells a ray may visit.
///
/// The reach is at most a few blocks, so this only guards against a caller
/// that asks for an absurd distance.
const MAX_VISITED_CELLS: u32 = 4096;

/// The face of a block a ray entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// The −X side.
    West,
    /// The +X side.
    East,
    /// The −Y side.
    Down,
    /// The +Y side.
    Up,
    /// The −Z side.
    North,
    /// The +Z side.
    South,
}

/// The result of a successful ray cast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// The block that was hit.
    pub block: BlockPos,
    /// The face of the block that the ray entered.
    pub face: Face,
    /// The exact point where the ray entered the block.
    pub position: DVec3,
    /// The distance from the ray origin to the hit.
    pub distance: f64,
}

/// The unit vector a player looks along.
///
/// MCP `net/minecraft/entity/Entity.java:1466` (`getVectorForRotation`)
/// builds it from the pitch and the yaw.
#[must_use]
pub fn look_direction(yaw: f32, pitch: f32) -> DVec3 {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    DVec3::new(
        -f64::from(sin_yaw) * f64::from(cos_pitch),
        -f64::from(sin_pitch),
        f64::from(cos_yaw) * f64::from(cos_pitch),
    )
}

/// Casts a ray through the voxel grid and returns the first block it hits.
///
/// The ray is limited to `max_distance`; blocks further away are ignored. A
/// ray that hits nothing returns `None`.
///
/// # Example
///
/// ```
/// use umicraft_game::raycast::{raycast, Face};
/// use umicraft_world::World;
///
/// let world = World::new(0);
/// // An empty world: the ray hits nothing.
/// assert!(raycast(
///     glam::DVec3::ZERO,
///     glam::DVec3::Y,
///     4.5,
///     &world,
/// )
/// .is_none());
/// ```
pub fn raycast<S: BlockSource + ?Sized>(
    origin: DVec3,
    direction: DVec3,
    max_distance: f64,
    world: &S,
) -> Option<RayHit> {
    let direction = direction.normalize();
    if direction == DVec3::ZERO || max_distance <= 0.0 {
        return None;
    }

    let mut cell = BlockPos::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let step_x = axis_step(direction.x);
    let step_y = axis_step(direction.y);
    let step_z = axis_step(direction.z);

    let delta_x = axis_delta(direction.x);
    let delta_y = axis_delta(direction.y);
    let delta_z = axis_delta(direction.z);

    let mut max_x = axis_max(origin.x, direction.x, cell.x);
    let mut max_y = axis_max(origin.y, direction.y, cell.y);
    let mut max_z = axis_max(origin.z, direction.z, cell.z);

    for _ in 0..MAX_VISITED_CELLS {
        let state = world.block(cell.x, cell.y, cell.z);
        if is_targetable(state) {
            let block_origin = DVec3::new(cell.x as f64, cell.y as f64, cell.z as f64);
            for local in selection_shape(state, world, cell).boxes() {
                let shape_box = Aabb::new(local.min + block_origin, local.max + block_origin);
                if let Some((distance, face)) = intersect(origin, direction, &shape_box) {
                    if distance <= max_distance {
                        return Some(RayHit {
                            block: cell,
                            face,
                            position: origin + direction * distance,
                            distance,
                        });
                    }
                }
            }
        }

        // Stop when the current cell already reaches beyond the ray.
        if max_x.min(max_y).min(max_z) > max_distance {
            return None;
        }

        // Advance to the next cell along the axis that is crossed first.
        if max_x < max_y && max_x < max_z {
            cell.x += step_x;
            max_x += delta_x;
        } else if max_y < max_z {
            cell.y += step_y;
            max_y += delta_y;
        } else {
            cell.z += step_z;
            max_z += delta_z;
        }
    }

    None
}

/// Whether the ray may stop at a block.
///
/// MCP `EntityLivingBase.rayTrace` calls `World.rayTraceBlocks(start, end,
/// false, ...)`: `stopOnLiquid` is false, so a ray looks through water and
/// lava and a liquid is never targeted, outlined or dug. Only the bucket
/// (`Item.getMovingObjectPositionFromPlayer`) asks for liquids.
fn is_targetable(state: umicraft_world::BlockState) -> bool {
    !matches!(
        state.kind(),
        umicraft_world::BlockKind::Air | umicraft_world::BlockKind::Liquid
    )
}

/// The step of the cell walk along one axis.
fn axis_step(direction: f64) -> i32 {
    if direction > 0.0 {
        1
    } else if direction < 0.0 {
        -1
    } else {
        0
    }
}

/// The parametric distance between two cell borders along one axis.
fn axis_delta(direction: f64) -> f64 {
    if direction == 0.0 {
        f64::INFINITY
    } else {
        1.0 / direction.abs()
    }
}

/// The parametric distance to the next cell border along one axis.
fn axis_max(origin: f64, direction: f64, cell: i32) -> f64 {
    if direction > 0.0 {
        (cell as f64 + 1.0 - origin) / direction
    } else if direction < 0.0 {
        (cell as f64 - origin) / direction
    } else {
        f64::INFINITY
    }
}

/// The intersection of a ray with a box.
///
/// Returns the distance and the face of the box the ray entered. The face
/// follows `AxisAlignedBB.calculateIntercept` (MCP
/// `net/minecraft/util/AxisAlignedBB.java:271`).
fn intersect(origin: DVec3, direction: DVec3, shape_box: &Aabb) -> Option<(f64, Face)> {
    let (x_near, x_far) = intersect_axis(origin.x, direction.x, shape_box.min.x, shape_box.max.x)?;
    let (y_near, y_far) = intersect_axis(origin.y, direction.y, shape_box.min.y, shape_box.max.y)?;
    let (z_near, z_far) = intersect_axis(origin.z, direction.z, shape_box.min.z, shape_box.max.z)?;

    let near = x_near.max(y_near).max(z_near);
    let far = x_far.min(y_far).min(z_far);
    if near > far || far < 0.0 {
        return None;
    }
    let distance = near.max(0.0);

    let face = if near == x_near {
        if direction.x > 0.0 {
            Face::West
        } else {
            Face::East
        }
    } else if near == y_near {
        if direction.y > 0.0 {
            Face::Down
        } else {
            Face::Up
        }
    } else if direction.z > 0.0 {
        Face::North
    } else {
        Face::South
    };

    Some((distance, face))
}

/// The parametric entry and exit of a ray along one axis.
fn intersect_axis(origin: f64, direction: f64, min: f64, max: f64) -> Option<(f64, f64)> {
    if direction.abs() < f64::EPSILON {
        if origin < min || origin > max {
            return None;
        }
        return Some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let inverse = 1.0 / direction;
    let first = (min - origin) * inverse;
    let second = (max - origin) * inverse;
    if first <= second {
        Some((first, second))
    } else {
        Some((second, first))
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

    #[test]
    fn the_ray_hits_the_block_in_front() {
        let world = TestBlocks::with(&[(0, 0, 3, BlockState::new(1, 0))]);
        let hit = raycast(DVec3::new(0.5, 0.5, 0.5), DVec3::Z, 4.5, &world)
            .expect("the ray must hit the block");
        assert_eq!(hit.block, BlockPos::new(0, 0, 3));
        assert_eq!(hit.face, Face::North);
        assert!((hit.distance - 2.5).abs() < 1e-9);
        assert!((hit.position.z - 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_ray_looks_through_liquids() {
        // Regression: water (8, 9) and lava (10, 11) used to be targeted.
        for id in 8..=11 {
            let world = TestBlocks::with(&[
                (0, 0, 2, BlockState::new(id, 0)),
                (0, 0, 3, BlockState::new(1, 0)),
            ]);
            let hit = raycast(DVec3::new(0.5, 0.5, 0.5), DVec3::Z, 4.5, &world)
                .expect("the ray must reach the stone behind the liquid");
            assert_eq!(hit.block, BlockPos::new(0, 0, 3));
        }
    }

    #[test]
    fn the_ray_reports_the_face_it_entered() {
        let world = TestBlocks::with(&[(0, 0, 0, BlockState::new(1, 0))]);
        // Looking down at the block from above.
        let hit = raycast(DVec3::new(0.5, 5.0, 0.5), DVec3::NEG_Y, 10.0, &world)
            .expect("the ray must hit the block");
        assert_eq!(hit.face, Face::Up);
        assert!((hit.distance - 4.0).abs() < 1e-9);
    }

    #[test]
    fn the_ray_stops_at_the_reach_distance() {
        let world = TestBlocks::with(&[(0, 0, 6, BlockState::new(1, 0))]);
        assert!(raycast(DVec3::new(0.5, 0.5, 0.5), DVec3::Z, 4.5, &world).is_none());
        assert!(raycast(DVec3::new(0.5, 0.5, 0.5), DVec3::Z, 6.0, &world).is_some());
    }

    #[test]
    fn the_ray_passes_through_air() {
        let world = TestBlocks::default();
        assert!(raycast(DVec3::ZERO, DVec3::Y, 4.5, &world).is_none());
    }

    #[test]
    fn the_look_direction_follows_the_yaw_and_pitch() {
        // Yaw 0 looks towards +Z, pitch 0 is the horizon.
        let direction = look_direction(0.0, 0.0);
        assert!((direction.z - 1.0).abs() < 1e-6);
        assert!(direction.x.abs() < 1e-6);

        // Looking straight down.
        let direction = look_direction(0.0, std::f32::consts::FRAC_PI_2);
        assert!((direction.y + 1.0).abs() < 1e-6);
    }
}
