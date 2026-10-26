// SPDX-License-Identifier: MIT OR Apache-2.0

//! Snowman (Snow Golem) box model matching MCP 1.8 `ModelSnowMan.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Snowman model (64×64 texture sheet) made of three stacked snow spheres and two stick arms.
#[derive(Debug, Clone)]
pub struct SnowmanModel {
    /// Head part (pumpkin or snow face).
    pub head: ModelPart,
    /// Middle body sphere.
    pub body: ModelPart,
    /// Bottom base sphere.
    pub bottom_body: ModelPart,
    /// Right stick arm.
    pub right_hand: ModelPart,
    /// Left stick arm.
    pub left_hand: ModelPart,
}

impl SnowmanModel {
    /// Creates a new Snowman model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 64.0];
        // In MCP: f = 4.0;
        // head: Y = 4.0. In Umicraft:
        // bottom_body: height 12, origin Y in [0, 12].
        // body: height 10, origin Y in [11, 21].
        // head: height 8, origin Y in [20, 28].
        let head_py = 20.0;
        let body_py = 11.0;
        let base_py = 0.0;

        let head = ModelPart::new([0.0, head_py, 0.0]).with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            -0.5,
        ));

        let body = ModelPart::new([0.0, body_py, 0.0]).with_box(ModelBox::new(
            [-5.0, 0.0, -5.0],
            [10.0, 10.0, 10.0],
            [0.0, 16.0],
            tex,
            -0.5,
        ));

        let bottom_body = ModelPart::new([0.0, base_py, 0.0]).with_box(ModelBox::new(
            [-6.0, 0.0, -6.0],
            [12.0, 12.0, 12.0],
            [0.0, 36.0],
            tex,
            -0.5,
        ));

        let right_hand = ModelPart::new([5.0, body_py + 6.0, 0.0]).with_box(ModelBox::new(
            [0.0, -1.0, -1.0],
            [12.0, 2.0, 2.0],
            [32.0, 0.0],
            tex,
            -0.5,
        ));

        let left_hand = ModelPart::new([-5.0, body_py + 6.0, 0.0]).with_box(ModelBox::new(
            [-12.0, -1.0, -1.0],
            [12.0, 2.0, 2.0],
            [32.0, 0.0],
            tex,
            -0.5,
        ));

        let mut sm = Self {
            head,
            body,
            bottom_body,
            right_hand,
            left_hand,
        };
        sm.right_hand.rotation[2] = -1.0;
        sm.left_hand.rotation[2] = 1.0;
        sm
    }

    /// Sets look rotation angles.
    pub fn set_angles(&mut self, head_yaw: f32, head_pitch: f32) {
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();
        self.body.rotation[1] = head_yaw.to_radians() * 0.25;
    }

    /// Emits all mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.bottom_body.emit(root, &mut out);
        self.right_hand.emit(root, &mut out);
        self.left_hand.emit(root, &mut out);
        out
    }
}

impl Default for SnowmanModel {
    fn default() -> Self {
        Self::new()
    }
}
