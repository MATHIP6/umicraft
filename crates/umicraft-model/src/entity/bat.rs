// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bat box model matching MCP 1.8 `ModelBat.java`.

use std::f32::consts::FRAC_PI_4;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Bat model (64×64 texture sheet) with flapping inner and outer wings.
#[derive(Debug, Clone)]
pub struct BatModel {
    /// Head part with ears.
    pub head: ModelPart,
    /// Body and tail part.
    pub body: ModelPart,
    /// Inner right wing.
    pub right_wing: ModelPart,
    /// Outer right wing (attached to inner right wing).
    pub outer_right_wing: ModelPart,
    /// Inner left wing.
    pub left_wing: ModelPart,
    /// Outer left wing (attached to inner left wing).
    pub outer_left_wing: ModelPart,
}

impl BatModel {
    /// Creates a new bat model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 64.0];

        // Head with ears (MCP ModelBat: head at [0, 0, 0], box [-3, -3, -3, 6, 6, 6], ears [-4, -6, -2, 3, 4, 1] and [1, -6, -2, 3, 4, 1])
        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-3.0, -3.0, -3.0],
                [6.0, 6.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 2.0, -2.0],
                [3.0, 4.0, 1.0],
                [24.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(
                ModelBox::new([1.0, 2.0, -2.0], [3.0, 4.0, 1.0], [24.0, 0.0], tex, 0.0).mirrored(),
            );

        // Body and tail membrane (MCP ModelBat: body at [0, 0, 0], box [-3, 4, -3, 6, 12, 6] and [-5, 16, 0, 10, 6, 1])
        let body = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-3.0, -16.0, -3.0],
                [6.0, 12.0, 6.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.0, -22.0, 0.0],
                [10.0, 6.0, 1.0],
                [0.0, 34.0],
                tex,
                0.0,
            ));

        // Inner wings (child of body at [0, 0, 0], box [-12, 1, 1.5, 10, 16, 1])
        let right_wing = ModelPart::new([0.0, 0.0, 0.0]).with_box(ModelBox::new(
            [-12.0, -17.0, 1.5],
            [10.0, 16.0, 1.0],
            [42.0, 0.0],
            tex,
            0.0,
        ));

        // Outer right wing (child of right wing at [-12, 1, 1.5], box [-8, 1, 0, 8, 12, 1])
        let outer_right_wing = ModelPart::new([-12.0, -1.0, 1.5]).with_box(ModelBox::new(
            [-8.0, -13.0, 0.0],
            [8.0, 12.0, 1.0],
            [24.0, 16.0],
            tex,
            0.0,
        ));

        // Inner left wing (mirror of right wing)
        let left_wing = ModelPart::new([0.0, 0.0, 0.0]).with_box(
            ModelBox::new([2.0, -17.0, 1.5], [10.0, 16.0, 1.0], [42.0, 0.0], tex, 0.0).mirrored(),
        );

        // Outer left wing (child of left wing at [12, 1, 1.5], box [0, 1, 0, 8, 12, 1])
        let outer_left_wing = ModelPart::new([12.0, -1.0, 1.5]).with_box(
            ModelBox::new([0.0, -13.0, 0.0], [8.0, 12.0, 1.0], [24.0, 16.0], tex, 0.0).mirrored(),
        );

        Self {
            head,
            body,
            right_wing,
            outer_right_wing,
            left_wing,
            outer_left_wing,
        }
    }

    /// Flaps the wings, tilts the body and turns the head of a flying bat
    /// (MCP `ModelBat.setRotationAngles`, the not-hanging branch).
    ///
    /// `age` is the ticks lived plus the partial tick (MCP `ageInTicks`): a
    /// bat flaps with time, not with walking. The head angles are in degrees.
    pub fn set_angles(&mut self, age: f32, head_yaw: f32, head_pitch: f32) {
        // Flipping y turns the sense of x rotations around (as in `pose.rs`).
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        // The body leans forward 45 degrees and rocks slowly.
        self.body.rotation[0] = -FRAC_PI_4 - (age * 0.1).cos() * 0.15;

        let flap = (age * 1.3).cos() * std::f32::consts::PI * 0.25;
        self.right_wing.rotation[1] = flap;
        self.left_wing.rotation[1] = -flap;
        self.outer_right_wing.rotation[1] = flap * 0.5;
        self.outer_left_wing.rotation[1] = -flap * 0.5;
    }

    /// Emits the bat mesh vertices transformed into world units (with MCP RenderBat 0.35x scale).
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 0.35 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.head.emit(root, &mut out);

        let body_transform = self.body.transform(root);
        self.body.emit(root, &mut out);

        let rw_transform = self.right_wing.transform(body_transform);
        self.right_wing.emit(body_transform, &mut out);
        self.outer_right_wing.emit(rw_transform, &mut out);

        let lw_transform = self.left_wing.transform(body_transform);
        self.left_wing.emit(body_transform, &mut out);
        self.outer_left_wing.emit(lw_transform, &mut out);

        out
    }
}

impl Default for BatModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wings_flap_with_age_alone() {
        // Regression: the wings were driven by the limb swing, which stays
        // still while a bat flies, so they never flapped.
        let mut early = BatModel::new();
        early.set_angles(0.0, 0.0, 0.0);
        let mut later = BatModel::new();
        later.set_angles(1.0, 0.0, 0.0);
        assert_ne!(early.right_wing.rotation, later.right_wing.rotation);
    }

    #[test]
    fn the_wings_start_at_the_mcp_phase() {
        // MCP `ModelBat`: the right wing turns by `cos(age * 1.3) * PI / 4`.
        let mut model = BatModel::new();
        model.set_angles(0.0, 0.0, 0.0);
        assert!((model.right_wing.rotation[1] - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert!((model.outer_left_wing.rotation[1] + std::f32::consts::FRAC_PI_8).abs() < 1e-6);
    }

    #[test]
    fn the_head_follows_the_look() {
        let mut model = BatModel::new();
        model.set_angles(0.0, 45.0, 20.0);
        assert!((model.head.rotation[1] - 45.0_f32.to_radians()).abs() < 1e-6);
        assert!((model.head.rotation[0] + 20.0_f32.to_radians()).abs() < 1e-6);
    }
}
