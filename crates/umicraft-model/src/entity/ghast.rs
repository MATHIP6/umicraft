// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ghast box model matching MCP 1.8 `ModelGhast.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// The factor `RenderGhast.preRenderCallback` enlarges the model by.
pub const RENDER_SCALE: f32 = 4.5;

/// The tentacle lengths: `new Random(1660L).nextInt(7) + 8`, nine times
/// (computed with the reference algorithm).
const TENTACLE_LENGTHS: [f32; 9] = [8.0, 13.0, 9.0, 11.0, 11.0, 10.0, 12.0, 9.0, 12.0];

/// Ghast model (64×32 texture sheet) with 1 large cubic body and 9 trailing tentacles.
#[derive(Debug, Clone)]
pub struct GhastModel {
    /// Large 16×16×16 body cube.
    pub body: ModelPart,
    /// 9 dangling animated tentacles.
    pub tentacles: [ModelPart; 9],
}

impl GhastModel {
    /// Creates a new ghast model, at the size `ModelGhast` builds it (the
    /// renderer enlarges it by [`RENDER_SCALE`]).
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        // `ModelGhast.render` moves everything 0.6 block down (the vanilla y
        // axis points down); the pivots below include that shift.
        let down = 9.6;
        let body = ModelPart::new([0.0, 24.0 - (8.0 + down), 0.0]).with_box(ModelBox::new(
            [-8.0, -8.0, -8.0],
            [16.0, 16.0, 16.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let tentacles = std::array::from_fn(|j| {
            let column = (j % 3) as f32;
            let row = (j / 3) as f32;
            let x = (column - (j / 3 % 2) as f32 * 0.5 + 0.25 - 1.0) * 5.0;
            let z = (row - 1.0) * 5.0;
            let length = TENTACLE_LENGTHS[j];
            ModelPart::new([x, 24.0 - (15.0 + down), z]).with_box(ModelBox::new(
                [-1.0, -length, -1.0],
                [2.0, length, 2.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
        });

        let mut model = Self { body, tentacles };
        model.set_angles(0.0);
        model
    }

    /// Sets the tentacle sway (MCP `ModelGhast.setRotationAngles`), with the
    /// age of the entity in ticks.
    pub fn set_angles(&mut self, age: f32) {
        for (j, tentacle) in self.tentacles.iter_mut().enumerate() {
            tentacle.rotation[0] = -(0.2 * (age * 0.3 + j as f32).sin() + 0.4);
        }
    }

    /// Emits the ghast mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.body.emit(root, &mut out);
        for tentacle in &self.tentacles {
            tentacle.emit(root, &mut out);
        }

        out
    }
}

impl Default for GhastModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_has_a_body_and_nine_tentacles_of_the_reference_lengths() {
        let model = GhastModel::new();
        assert_eq!(model.emit_mesh().len(), 10 * 36);
        let lengths: Vec<f32> = model.tentacles.iter().map(|t| t.boxes[0].size[1]).collect();
        assert_eq!(lengths, TENTACLE_LENGTHS.to_vec());
        assert!(lengths.iter().all(|l| (8.0..=14.0).contains(l)));
    }

    #[test]
    fn the_tentacles_sway_with_the_age_and_each_has_its_own_phase() {
        let mut model = GhastModel::new();
        model.set_angles(0.0);
        let first = model.tentacles[0].rotation[0];
        model.set_angles(10.0);
        assert_ne!(model.tentacles[0].rotation[0], first);
        assert_ne!(
            model.tentacles[0].rotation[0],
            model.tentacles[1].rotation[0]
        );
    }

    #[test]
    fn the_renderer_makes_the_ghast_four_and_a_half_times_larger() {
        assert_eq!(RENDER_SCALE, 4.5);
        let model = GhastModel::new();
        let xs: Vec<f32> = model.emit_mesh().iter().map(|v| v.position[0]).collect();
        let width = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        // The body is a block wide; scaled it is the 4.5 blocks of the hitbox
        // the real ghast has.
        assert!((width * RENDER_SCALE - 4.5).abs() < 0.5, "{width}");
    }
}
