// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chest model (MCP `ModelChest` and `ModelLargeChest`).
//!
//! `TileEntityChestRenderer.renderTileEntityAt` draws a chest in the block
//! space of its tile: the lid and the knob pivot together, so a chest is three
//! boxes. A chest minecart draws the same model with no world (shut, facing
//! south). The ender chest and the trapped chest use the same geometry with
//! other textures.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// The `GlStateManager.rotate` angle, in degrees, with which
/// `TileEntityChestRenderer` turns a chest of the given facing metadata
/// (2 north, 3 south, 4 west, 5 east; anything else draws as south, the
/// reference's default of `j = 0`).
///
/// The reference writes east as `-90`; `270` is the same turn.
#[must_use]
pub fn chest_facing_degrees(metadata: u8) -> f32 {
    match metadata {
        2 => 180.0,
        4 => 90.0,
        5 => 270.0,
        _ => 0.0,
    }
}

/// A chest of one block (64x64 sheet) or two (128x64 sheet).
#[derive(Debug, Clone, Copy, Default)]
pub struct ChestModel;

impl ChestModel {
    /// The shut chest in the block space of one block, `0..1` on each axis,
    /// as a chest minecart carries it: facing south (angle 0).
    #[must_use]
    pub fn emit_mesh() -> Vec<ModelVertex> {
        Self::emit_at(0.0, false, 0.0)
    }

    /// The chest in the block space of its tile, `TileEntityChestRenderer
    /// .renderTileEntityAt` for a tile with a world.
    ///
    /// `lid_angle` is the eased lid rotation in radians exactly as the
    /// renderer sets `chestLid.rotateAngleX` (`-(f * PI / 2)`, so `0` is shut
    /// and negative opens); the knob copies it (`ModelChest.renderAll`).
    /// `large` draws `ModelLargeChest`, which spans two blocks along `+X`
    /// before the facing turn, anchored at the origin block (the tile with no
    /// neighbour on its negative side). `facing_degrees` is the renderer's
    /// `rotate(j, 0, 1, 0)` about the block centre, see
    /// [`chest_facing_degrees`]; the two shifts the renderer applies to a
    /// large chest facing north (`+1` in x) and east (`-1` in its flipped z)
    /// keep the pair on the blocks it covers.
    #[must_use]
    pub fn emit_at(lid_angle: f32, large: bool, facing_degrees: f32) -> Vec<ModelVertex> {
        let (tex, width) = if large {
            ([128.0, 64.0], 30.0)
        } else {
            ([64.0, 64.0], 14.0)
        };
        // The reference's pivots and boxes are y-down; the crate's are y-up:
        // pivot `(x, -y, z)`, box origin `-(y + h)`, and a rotation about x
        // turns the other way.
        let part = |pivot: [f32; 3], origin: [f32; 3], size: [f32; 3], uv: [f32; 2]| {
            ModelPart::new([pivot[0], -pivot[1], pivot[2]]).with_box(ModelBox::new(
                [origin[0], -(origin[1] + size[1]), origin[2]],
                size,
                uv,
                tex,
                0.0,
            ))
        };
        let mut lid = part(
            [1.0, 7.0, 15.0],
            [0.0, -5.0, -14.0],
            [width, 5.0, 14.0],
            [0.0, 0.0],
        );
        let mut knob = part(
            [if large { 16.0 } else { 8.0 }, 7.0, 15.0],
            [-1.0, -2.0, -15.0],
            [2.0, 4.0, 1.0],
            [0.0, 0.0],
        );
        lid.rotation[0] = -lid_angle;
        knob.rotation[0] = -lid_angle;
        let base = part(
            [1.0, 6.0, 1.0],
            [0.0, 0.0, 0.0],
            [width, 10.0, 14.0],
            [0.0, 19.0],
        );
        let root = Self::root(large, facing_degrees);
        let mut out = Vec::new();
        for part in [&lid, &knob, &base] {
            part.emit(root, &mut out);
        }
        out
    }

    /// The renderer's matrix stack, `translate(0, 1, 1)`, `scale(1, -1, -1)`,
    /// `translate(.5)`, the large-chest shift, `rotate(j, y)`,
    /// `translate(-.5)`, then the model in sixteenths; the model's own y-down
    /// is already undone by the y-up boxes, hence `-1/16` on y.
    fn root(large: bool, facing_degrees: f32) -> Mat4 {
        let turn = facing_degrees.rem_euclid(360.0);
        let shift = if !large {
            Vec3::ZERO
        } else if (turn - 180.0).abs() < 1e-3 {
            Vec3::X
        } else if (turn - 270.0).abs() < 1e-3 {
            -Vec3::Z
        } else {
            Vec3::ZERO
        };
        let half = Vec3::splat(0.5);
        Mat4::from_translation(Vec3::new(0.0, 1.0, 1.0))
            * Mat4::from_scale(Vec3::new(1.0, -1.0, -1.0))
            * Mat4::from_translation(half + shift)
            * Mat4::from_rotation_y(facing_degrees.to_radians())
            * Mat4::from_translation(-half)
            * Mat4::from_scale(Vec3::new(1.0 / 16.0, -1.0 / 16.0, 1.0 / 16.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(vertices: &[ModelVertex], axis: usize) -> (f32, f32) {
        vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), v| {
                (low.min(v.position[axis]), high.max(v.position[axis]))
            })
    }

    #[test]
    fn a_shut_chest_is_three_boxes_inside_its_block() {
        let chest = ChestModel::emit_mesh();
        assert_eq!(chest.len(), 3 * 36);
        for axis in 0..3 {
            let (low, high) = extent(&chest, axis);
            assert!(low >= -1e-4 && high <= 1.0001, "axis {axis}: {low} {high}");
        }
        // 14 pixels wide, 14 high: a base of 10 under a lid of 5, which overlap by 1.
        let (low, high) = extent(&chest, 0);
        assert!((high - low - 14.0 / 16.0).abs() < 1e-4);
        let (low, high) = extent(&chest, 1);
        assert!(
            low.abs() < 1e-4 && (high - 14.0 / 16.0).abs() < 1e-4,
            "{low} {high}"
        );
    }

    #[test]
    fn the_knob_sits_on_the_south_face_of_the_lid() {
        let chest = ChestModel::emit_mesh();
        let reach = |boxes: std::ops::Range<usize>| {
            chest[boxes]
                .iter()
                .map(|v| v.position[2])
                .fold(f32::MIN, f32::max)
        };
        // The knob (the second box) reaches 1/16 further than the lid.
        assert!(reach(36..72) > reach(0..36) + 0.05);
    }

    fn assert_near(a: &[ModelVertex], b: &[ModelVertex]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            for axis in 0..3 {
                assert!((x.position[axis] - y.position[axis]).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn the_shut_chest_facing_south_is_the_minecart_mesh() {
        // Pinned positions of the lid's first corner and the base's extent.
        let chest = ChestModel::emit_at(0.0, false, chest_facing_degrees(3));
        assert_near(&chest, &ChestModel::emit_mesh());
        let (low, high) = extent(&chest[72..], 1);
        assert!(low.abs() < 1e-4 && (high - 10.0 / 16.0).abs() < 1e-4);
    }

    #[test]
    fn opening_the_lid_moves_only_the_lid_and_the_knob() {
        let shut = ChestModel::emit_at(0.0, false, 0.0);
        let open = ChestModel::emit_at(-1.0, false, 0.0);
        assert_near(&shut[72..], &open[72..]);
        let top = |v: &[ModelVertex]| extent(&v[..72], 1).1;
        assert!(top(&open) > top(&shut) + 0.1);
        assert!(shut[..72]
            .iter()
            .zip(&open[..72])
            .any(|(a, b)| (a.position[1] - b.position[1]).abs() > 0.05));
    }

    #[test]
    fn a_large_chest_is_two_blocks_wide() {
        let chest = ChestModel::emit_at(0.0, true, 0.0);
        let (low, high) = extent(&chest, 0);
        assert!((low - 1.0 / 16.0).abs() < 1e-4);
        assert!((high - (31.0 / 16.0)).abs() < 1e-4, "{high}");
        assert!((extent(&chest, 2).1 - extent(&ChestModel::emit_mesh(), 2).1).abs() < 1e-4);
    }

    #[test]
    fn a_facing_turns_about_the_block_centre() {
        for metadata in 2..=5 {
            let degrees = chest_facing_degrees(metadata);
            let chest = ChestModel::emit_at(0.0, false, degrees);
            for axis in [0, 2] {
                let (low, high) = extent(&chest, axis);
                assert!(low >= -0.001 && high <= 1.001, "{metadata} {axis}");
            }
            // The centre of the square footprint is the block centre.
            let (low, high) = extent(&chest[72..], 0);
            assert!(((low + high) / 2.0 - 0.5).abs() < 1e-3);
        }
    }

    #[test]
    fn a_large_chest_stays_on_its_two_blocks_for_each_facing() {
        for (metadata, along_x) in [(2, true), (3, true), (4, false), (5, false)] {
            let chest = ChestModel::emit_at(0.0, true, chest_facing_degrees(metadata));
            let (long, short) = if along_x { (0, 2) } else { (2, 0) };
            let (low, high) = extent(&chest, long);
            assert!(high - low > 1.8, "{metadata}");
            assert!(low >= -0.001 && high <= 2.001, "{metadata}: {low} {high}");
            let (low, high) = extent(&chest, short);
            assert!(low >= -0.001 && high <= 1.001, "{metadata}: {low} {high}");
        }
    }

    #[test]
    fn facing_angles_follow_the_renderer() {
        let angles: Vec<f32> = (2..=5).map(chest_facing_degrees).collect();
        assert_eq!(angles, [180.0, 0.0, 90.0, 270.0]);
    }
}
