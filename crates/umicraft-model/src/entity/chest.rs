// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chest model (MCP `ModelChest`) as a minecart carries it.
//!
//! A chest minecart draws the chest through `TileEntityChestRenderer` with no
//! world: facing south, the lid shut, in the block space of one block. The
//! lid and the knob pivot together, so a shut chest is three boxes.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// A chest of one block, 64x64 texture sheet.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChestModel;

impl ChestModel {
    /// The shut chest in the block space of one block, `0..1` on each axis
    /// (`TileEntityChestRenderer.renderTileEntityAt` for a tile without a
    /// world: `translate(0, 1, 1)`, `scale(1, -1, -1)`, then the model in
    /// sixteenths of a block with its y axis down).
    #[must_use]
    pub fn emit_mesh() -> Vec<ModelVertex> {
        let tex = [64.0, 64.0];
        // The reference's pivots and boxes are y-down; the crate's are y-up:
        // pivot `(x, 24 - y, z)`, box origin `-(y + h)`.
        let part = |pivot: [f32; 3], origin: [f32; 3], size: [f32; 3], uv: [f32; 2]| {
            ModelPart::new([pivot[0], 24.0 - pivot[1], pivot[2]]).with_box(ModelBox::new(
                [origin[0], -(origin[1] + size[1]), origin[2]],
                size,
                uv,
                tex,
                0.0,
            ))
        };
        let parts = [
            // The lid and the knob, shut.
            part(
                [1.0, 7.0, 15.0],
                [0.0, -5.0, -14.0],
                [14.0, 5.0, 14.0],
                [0.0, 0.0],
            ),
            part(
                [8.0, 7.0, 15.0],
                [-1.0, -2.0, -15.0],
                [2.0, 4.0, 1.0],
                [0.0, 0.0],
            ),
            // The base.
            part(
                [1.0, 6.0, 1.0],
                [0.0, 0.0, 0.0],
                [14.0, 10.0, 14.0],
                [0.0, 19.0],
            ),
        ];
        // The crate's y is `24 - y` of the reference, in sixteenths; the block
        // space is `(x, 1 - y, 1 - z)` of the reference's blocks.
        let root = Mat4::from_translation(Vec3::new(0.0, -0.5, 1.0))
            * Mat4::from_scale(Vec3::new(1.0 / 16.0, 1.0 / 16.0, -1.0 / 16.0));
        let mut out = Vec::new();
        for part in &parts {
            part.emit(root, &mut out);
        }
        out
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
}
