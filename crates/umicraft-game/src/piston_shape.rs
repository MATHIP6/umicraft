// SPDX-License-Identifier: MIT OR Apache-2.0

//! Collision boxes of the piston blocks.
//!
//! Source: MCP `BlockPistonBase.setBlockBoundsBasedOnState` (an extended base
//! is 12/16 deep, the head standing in the rest of the way) and
//! `BlockPistonExtension.applyHeadBounds` / `applyCoreBounds` /
//! `addCollisionBoxesToList` (the head is a 4/16 plate at the end plus a
//! 4/16-wide rod back to the base).

use glam::DVec3;

use crate::shapes::{Aabb, BlockShape};

/// A box from two corners given as arrays.
fn bounds(min: [f64; 3], max: [f64; 3]) -> Aabb {
    Aabb::new(DVec3::from_array(min), DVec3::from_array(max))
}

/// The collision shape of a piston base (`BlockPistonBase`), from its
/// metadata: bits 0..=2 are the facing (0 down, 1 up, 2 north, 3 south,
/// 4 west, 5 east), bit 3 is `extended`.
#[must_use]
pub(crate) fn base_shape(metadata: u8) -> BlockShape {
    if metadata & 8 == 0 {
        return BlockShape::cube();
    }
    let (min, max) = match metadata & 7 {
        0 => ([0.0, 0.25, 0.0], [1.0, 1.0, 1.0]),
        1 => ([0.0, 0.0, 0.0], [1.0, 0.75, 1.0]),
        2 => ([0.0, 0.0, 0.25], [1.0, 1.0, 1.0]),
        3 => ([0.0, 0.0, 0.0], [1.0, 1.0, 0.75]),
        4 => ([0.25, 0.0, 0.0], [1.0, 1.0, 1.0]),
        _ => ([0.0, 0.0, 0.0], [0.75, 1.0, 1.0]),
    };
    BlockShape::single(bounds(min, max))
}

/// The collision shape of a piston head (`BlockPistonExtension`): the plate,
/// then the rod. An unknown facing (6, 7) draws like down, as the registry
/// folds it.
#[must_use]
pub(crate) fn head_shape(metadata: u8) -> BlockShape {
    let (plate, rod) = match metadata & 7 {
        1 => (
            ([0.0, 0.75, 0.0], [1.0, 1.0, 1.0]),
            ([0.375, 0.0, 0.375], [0.625, 0.75, 0.625]),
        ),
        2 => (
            ([0.0, 0.0, 0.0], [1.0, 1.0, 0.25]),
            ([0.25, 0.375, 0.25], [0.75, 0.625, 1.0]),
        ),
        3 => (
            ([0.0, 0.0, 0.75], [1.0, 1.0, 1.0]),
            ([0.25, 0.375, 0.0], [0.75, 0.625, 0.75]),
        ),
        4 => (
            ([0.0, 0.0, 0.0], [0.25, 1.0, 1.0]),
            ([0.375, 0.25, 0.25], [0.625, 0.75, 1.0]),
        ),
        5 => (
            ([0.75, 0.0, 0.0], [1.0, 1.0, 1.0]),
            ([0.0, 0.375, 0.25], [0.75, 0.625, 0.75]),
        ),
        _ => (
            ([0.0, 0.0, 0.0], [1.0, 0.25, 1.0]),
            ([0.375, 0.25, 0.375], [0.625, 1.0, 0.625]),
        ),
    };
    BlockShape::pair(bounds(plate.0, plate.1), bounds(rod.0, rod.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retracted_base_is_a_full_cube() {
        assert_eq!(base_shape(1).boxes(), BlockShape::cube().boxes());
    }

    #[test]
    fn an_extended_base_gives_up_the_quarter_the_head_stands_in() {
        let up = base_shape(1 | 8);
        assert_eq!(up.boxes()[0].max.y, 0.75);
        let west = base_shape(4 | 8);
        assert_eq!(west.boxes()[0].min.x, 0.25);
    }

    #[test]
    fn a_head_is_a_plate_and_a_rod_reaching_back_to_the_base() {
        let up = head_shape(1);
        let [plate, rod] = [up.boxes()[0], up.boxes()[1]];
        assert_eq!((plate.min.y, plate.max.y), (0.75, 1.0));
        assert_eq!((rod.min.y, rod.max.y), (0.0, 0.75));
        let north = head_shape(2);
        assert_eq!(north.boxes()[0].max.z, 0.25);
        assert_eq!(north.boxes()[1].max.z, 1.0);
    }
}
