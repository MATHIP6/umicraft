// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cape of a player (MCP `ModelPlayer.bipedCape`, `LayerCape`).

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Offset of the cape behind the back, in model units (`translate(0, 0, 0.125)`).
const BACK_OFFSET: f32 = 2.0;

/// A cape: one 10 x 16 x 1 box on the 64 x 32 cape texture.
#[derive(Debug, Clone)]
pub struct CapeModel {
    part: ModelPart,
}

impl Default for CapeModel {
    fn default() -> Self {
        Self::new()
    }
}

impl CapeModel {
    /// Builds the cape.
    #[must_use]
    pub fn new() -> Self {
        let part = ModelPart::new([0.0, 0.0, 0.0]).with_box(ModelBox::new(
            [-5.0, -16.0, -1.0],
            [10.0, 16.0, 1.0],
            [0.0, 0.0],
            [64.0, 32.0],
            0.0,
        ));
        Self { part }
    }

    /// Emits the cape hanging from the shoulders (`y = 24` units).
    ///
    /// `pitch` and `roll` are the vanilla swing angles in degrees
    /// (MCP `LayerCape`: `rotate(pitch, x)`, `rotate(roll, z)`,
    /// `rotate(-roll, y)`, `rotate(180, y)`); they are converted to the y-up
    /// model space here. A crouching player's cape hangs from two units
    /// lower (`ModelPlayer`: `bipedCape.rotationPointY = 2`).
    #[must_use]
    pub fn emit_mesh(&self, pitch: f32, roll: f32, sneaking: bool) -> Vec<ModelVertex> {
        let mut part = self.part.clone();
        part.pivot[1] = if sneaking { -2.0 } else { 0.0 };
        let chain = Mat4::from_translation(Vec3::new(0.0, 24.0, BACK_OFFSET))
            * Mat4::from_rotation_x(-pitch.to_radians())
            * Mat4::from_rotation_z(-roll.to_radians())
            * Mat4::from_rotation_y(-roll.to_radians())
            * Mat4::from_rotation_y(PI);
        let mut out = Vec::new();
        part.emit(Mat4::from_scale(Vec3::splat(1.0 / 16.0)) * chain, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centre(vertices: &[ModelVertex]) -> Vec3 {
        let sum: Vec3 = vertices.iter().map(|v| Vec3::from(v.position)).sum();
        sum / vertices.len() as f32
    }

    #[test]
    fn a_cape_is_one_box() {
        assert_eq!(CapeModel::new().emit_mesh(0.0, 0.0, false).len(), 36);
    }

    #[test]
    fn a_hanging_cape_is_behind_the_back_and_reaches_the_hips() {
        let mesh = CapeModel::new().emit_mesh(0.0, 0.0, false);
        let middle = centre(&mesh);
        // The model faces -Z: behind is +Z. The box is 16 units tall and
        // hangs from the shoulders at 1.5 blocks.
        assert!(middle.z > 0.05, "{middle}");
        assert!((middle.y - (24.0 - 8.0) / 16.0).abs() < 0.05, "{middle}");
    }

    #[test]
    fn lifting_the_hem_swings_the_cape_away_from_the_back() {
        let still = centre(&CapeModel::new().emit_mesh(0.0, 0.0, false));
        let lifted = centre(&CapeModel::new().emit_mesh(40.0, 0.0, false));
        assert!(lifted.z > still.z + 0.1, "{lifted} {still}");
        assert!(lifted.y > still.y, "the hem rises");
    }

    #[test]
    fn a_crouching_cape_hangs_lower() {
        let standing = centre(&CapeModel::new().emit_mesh(0.0, 0.0, false));
        let crouching = centre(&CapeModel::new().emit_mesh(0.0, 0.0, true));
        assert!(crouching.y < standing.y);
    }
}
