// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bat box model matching MCP 1.8 `ModelBat.java`.

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

        // Head with ears
        let head = ModelPart::new([0.0, 10.0, 0.0])
            .with_box(ModelBox::new(
                [-3.0, -3.0, -3.0],
                [6.0, 6.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 3.0, -1.0],
                [3.0, 4.0, 1.0],
                [24.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [1.0, 3.0, -1.0],
                [3.0, 4.0, 1.0],
                [24.0, 0.0],
                tex,
                0.0,
            ));

        // Body and tail membrane
        let body = ModelPart::new([0.0, 6.0, 0.0])
            .with_box(ModelBox::new(
                [-3.0, -6.0, -3.0],
                [6.0, 12.0, 6.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.0, -12.0, 0.0],
                [10.0, 6.0, 1.0],
                [0.0, 34.0],
                tex,
                0.0,
            ));

        // Inner wings
        let right_wing = ModelPart::new([0.0, 6.0, 0.0]).with_box(ModelBox::new(
            [-12.0, -8.0, 0.0],
            [10.0, 16.0, 1.0],
            [42.0, 0.0],
            tex,
            0.0,
        ));

        let outer_right_wing = ModelPart::new([-12.0, 6.0, 0.0]).with_box(ModelBox::new(
            [-8.0, -6.0, 0.0],
            [8.0, 12.0, 1.0],
            [24.0, 16.0],
            tex,
            0.0,
        ));

        let left_wing = ModelPart::new([0.0, 6.0, 0.0]).with_box(ModelBox::new(
            [2.0, -8.0, 0.0],
            [10.0, 16.0, 1.0],
            [42.0, 0.0],
            tex,
            0.0,
        ));

        let outer_left_wing = ModelPart::new([12.0, 6.0, 0.0]).with_box(ModelBox::new(
            [0.0, -6.0, 0.0],
            [8.0, 12.0, 1.0],
            [24.0, 16.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            right_wing,
            outer_right_wing,
            left_wing,
            outer_left_wing,
        }
    }

    /// Sets the wing flapping animation angles.
    pub fn set_angles(&mut self, anim_time: f32) {
        let flap = (anim_time * 1.3).cos() * std::f32::consts::PI * 0.25;
        self.right_wing.rotation[1] = flap;
        self.left_wing.rotation[1] = -flap;
        self.outer_right_wing.rotation[1] = flap * 0.5;
        self.outer_left_wing.rotation[1] = -flap * 0.5;
    }

    /// Emits the bat mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.right_wing.emit(root, &mut out);
        self.outer_right_wing.emit(root, &mut out);
        self.left_wing.emit(root, &mut out);
        self.outer_left_wing.emit(root, &mut out);

        out
    }
}

impl Default for BatModel {
    fn default() -> Self {
        Self::new()
    }
}
