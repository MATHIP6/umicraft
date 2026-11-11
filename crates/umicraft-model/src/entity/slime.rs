// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slime and Magma Cube box models matching MCP 1.8 `ModelSlime.java` and `ModelMagmaCube.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Slime model with inner core, eyes, mouth, and outer translucent gel layer.
#[derive(Debug, Clone)]
pub struct SlimeModel {
    /// Inner core body.
    pub inner_body: ModelPart,
    /// Outer gel layer.
    pub outer_body: ModelPart,
    /// Right eye.
    pub right_eye: ModelPart,
    /// Left eye.
    pub left_eye: ModelPart,
    /// Mouth.
    pub mouth: ModelPart,
    /// Scale factor (1.0 for small, 2.0 for medium, 4.0 for big).
    pub scale: f32,
    /// Is magma cube.
    pub is_magma: bool,
    /// The eight slices of a magma cube's shell, top first (empty parts for
    /// a slime).
    pub segments: [ModelPart; 8],
}

impl SlimeModel {
    /// Creates a slime model.
    #[must_use]
    pub fn slime(scale: f32) -> Self {
        let tex = [64.0, 32.0];
        // Ground is Y=0. Height is 8.
        let py = 0.0;

        let outer_body = ModelPart::new([0.0, py, 0.0]).with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let inner_body = ModelPart::new([0.0, py, 0.0]).with_box(ModelBox::new(
            [-3.0, 1.0, -3.0],
            [6.0, 6.0, 6.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        let right_eye = ModelPart::new([0.0, py, 0.0]).with_box(ModelBox::new(
            [-3.25, 4.0, -3.5],
            [2.0, 2.0, 2.0],
            [32.0, 0.0],
            tex,
            0.0,
        ));

        let left_eye = ModelPart::new([0.0, py, 0.0]).with_box(ModelBox::new(
            [1.25, 4.0, -3.5],
            [2.0, 2.0, 2.0],
            [32.0, 4.0],
            tex,
            0.0,
        ));

        let mouth = ModelPart::new([0.0, py, 0.0]).with_box(ModelBox::new(
            [0.0, 2.0, -3.5],
            [1.0, 1.0, 1.0],
            [32.0, 8.0],
            tex,
            0.0,
        ));

        Self {
            inner_body,
            outer_body,
            right_eye,
            left_eye,
            mouth,
            scale,
            is_magma: false,
            segments: std::array::from_fn(|_| ModelPart::new([0.0; 3])),
        }
    }

    /// Creates a magma cube model (segmented 8-layer shell + inner core) matching MCP `ModelMagmaCube.java`.
    #[must_use]
    pub fn magma_cube(scale: f32) -> Self {
        let tex = [64.0, 32.0];
        // 8 slices from MCP ModelMagmaCube: slice `i` is at y `16 + i`
        // (24 is the ground), so slice 0 is at 7 here and slice 7 at 0.
        let segments = std::array::from_fn(|i| {
            let (u, v) = match i {
                2 => (24.0, 10.0),
                3 => (24.0, 19.0),
                other => (0.0, other as f32),
            };
            ModelPart::new([0.0; 3]).with_box(ModelBox::new(
                [-4.0, (7 - i) as f32, -4.0],
                [8.0, 1.0, 8.0],
                [u, v],
                tex,
                0.0,
            ))
        });

        // Inner core (4x4x4) at [0, 16] in MCP at Y=18..22 (Umicraft Y=2..6)
        let core = ModelPart::new([0.0; 3]).with_box(ModelBox::new(
            [-2.0, 2.0, -2.0],
            [4.0, 4.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        Self {
            inner_body: core,
            outer_body: ModelPart::new([0.0, 0.0, 0.0]),
            right_eye: ModelPart::new([0.0, 0.0, 0.0]),
            left_eye: ModelPart::new([0.0, 0.0, 0.0]),
            mouth: ModelPart::new([0.0, 0.0, 0.0]),
            scale,
            is_magma: true,
            segments,
        }
    }

    /// Spreads a magma cube's slices apart as it stretches in a jump (MCP
    /// `ModelMagmaCube.setLivingAnimations`): slice `i` rises by `(4 - i) *
    /// squish * 1.7` pixels, so the top ones fly up and the bottom ones drop.
    /// A squashed cube (`squish < 0`) keeps its slices together.
    pub fn set_squish(&mut self, squish: f32) {
        let stretch = squish.max(0.0);
        for (i, segment) in self.segments.iter_mut().enumerate() {
            segment.pivot[1] = (4.0 - i as f32) * stretch * 1.7;
        }
    }

    /// Emits the opaque inner mesh.
    #[must_use]
    pub fn emit_inner_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(self.scale / 16.0));
        self.inner_body.emit(root, &mut out);
        for segment in &self.segments {
            segment.emit(root, &mut out);
        }
        if !self.is_magma {
            self.right_eye.emit(root, &mut out);
            self.left_eye.emit(root, &mut out);
            self.mouth.emit(root, &mut out);
        }
        out
    }

    /// Emits the translucent outer gel layer (Slime only).
    #[must_use]
    pub fn emit_outer_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        if !self.is_magma {
            let root = Mat4::from_scale(Vec3::splat(self.scale / 16.0));
            self.outer_body.emit(root, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jumping_magma_cube_spreads_its_slices() {
        // Regression: the slices were one solid block, so the cube never
        // opened up in a jump.
        let mut cube = SlimeModel::magma_cube(1.0);
        let rest = cube.emit_inner_mesh();
        cube.set_squish(1.0);
        assert!((cube.segments[0].pivot[1] - 6.8).abs() < 1e-6);
        assert_eq!(cube.segments[4].pivot[1], 0.0);
        assert!((cube.segments[7].pivot[1] + 5.1).abs() < 1e-5);
        assert_ne!(cube.emit_inner_mesh(), rest);
        cube.set_squish(-0.5);
        assert_eq!(cube.emit_inner_mesh(), rest, "squashed, it stays shut");
    }
}
