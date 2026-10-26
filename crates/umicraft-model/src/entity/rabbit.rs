// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rabbit box model matching MCP 1.8 `ModelRabbit.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Rabbit model (64×32 texture sheet) with long ears, crouching thighs and hind feet.
#[derive(Debug, Clone)]
pub struct RabbitModel {
    /// Left hind foot.
    pub left_foot: ModelPart,
    /// Right hind foot.
    pub right_foot: ModelPart,
    /// Left hind thigh.
    pub left_thigh: ModelPart,
    /// Right hind thigh.
    pub right_thigh: ModelPart,
    /// Torso / body.
    pub body: ModelPart,
    /// Left front arm.
    pub left_arm: ModelPart,
    /// Right front arm.
    pub right_arm: ModelPart,
    /// Head part with muzzle/nose.
    pub head: ModelPart,
    /// Right upright ear.
    pub right_ear: ModelPart,
    /// Left upright ear.
    pub left_ear: ModelPart,
    /// Small tail puff.
    pub tail: ModelPart,
}

impl RabbitModel {
    /// Creates a new rabbit model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];

        // Feet (2×1×7)
        let left_foot = ModelPart::new([3.0, 0.0, 3.7]).with_box(ModelBox::new(
            [-1.0, 0.0, -3.7],
            [2.0, 1.0, 7.0],
            [26.0, 24.0],
            tex,
            0.0,
        ));

        let right_foot = ModelPart::new([-3.0, 0.0, 3.7]).with_box(ModelBox::new(
            [-1.0, 0.0, -3.7],
            [2.0, 1.0, 7.0],
            [8.0, 24.0],
            tex,
            0.0,
        ));

        // Thighs (2×4×5)
        let mut left_thigh = ModelPart::new([3.0, 1.0, 3.7]).with_box(ModelBox::new(
            [-1.0, 0.0, 0.0],
            [2.0, 4.0, 5.0],
            [30.0, 15.0],
            tex,
            0.0,
        ));
        left_thigh.rotation[0] = 0.349;

        let mut right_thigh = ModelPart::new([-3.0, 1.0, 3.7]).with_box(ModelBox::new(
            [-1.0, 0.0, 0.0],
            [2.0, 4.0, 5.0],
            [16.0, 15.0],
            tex,
            0.0,
        ));
        right_thigh.rotation[0] = 0.349;

        // Body
        let mut body = ModelPart::new([0.0, 5.0, 8.0]).with_box(ModelBox::new(
            [-3.0, -2.5, -10.0],
            [6.0, 5.0, 10.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));
        body.rotation[0] = 0.349;

        // Front arms
        let mut left_arm = ModelPart::new([3.0, 7.0, -1.0]).with_box(ModelBox::new(
            [-1.0, -7.0, -1.0],
            [2.0, 7.0, 2.0],
            [8.0, 15.0],
            tex,
            0.0,
        ));
        left_arm.rotation[0] = 0.174;

        let mut right_arm = ModelPart::new([-3.0, 7.0, -1.0]).with_box(ModelBox::new(
            [-1.0, -7.0, -1.0],
            [2.0, 7.0, 2.0],
            [0.0, 15.0],
            tex,
            0.0,
        ));
        right_arm.rotation[0] = 0.174;

        // Head (5×4×5) + Nose (1×1×1)
        let head = ModelPart::new([0.0, 8.0, -1.0])
            .with_box(ModelBox::new(
                [-2.5, 0.0, -5.0],
                [5.0, 4.0, 5.0],
                [32.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-0.5, 1.5, -5.5],
                [1.0, 1.0, 1.0],
                [32.0, 9.0],
                tex,
                0.0,
            ));

        // Ears (2×5×1)
        let mut right_ear = ModelPart::new([0.0, 8.0, -1.0]).with_box(ModelBox::new(
            [-2.5, 4.0, -1.0],
            [2.0, 5.0, 1.0],
            [52.0, 0.0],
            tex,
            0.0,
        ));
        right_ear.rotation[1] = -0.261;

        let mut left_ear = ModelPart::new([0.0, 8.0, -1.0]).with_box(ModelBox::new(
            [0.5, 4.0, -1.0],
            [2.0, 5.0, 1.0],
            [58.0, 0.0],
            tex,
            0.0,
        ));
        left_ear.rotation[1] = 0.261;

        // Tail (3×3×2)
        let mut tail = ModelPart::new([0.0, 4.0, 7.0]).with_box(ModelBox::new(
            [-1.5, -1.5, 0.0],
            [3.0, 3.0, 2.0],
            [52.0, 6.0],
            tex,
            0.0,
        ));
        tail.rotation[0] = 0.349;

        Self {
            left_foot,
            right_foot,
            left_thigh,
            right_thigh,
            body,
            left_arm,
            right_arm,
            head,
            right_ear,
            left_ear,
            tail,
        }
    }

    /// Sets hopping walk and look animation angles.
    pub fn set_angles(
        &mut self,
        limb_angle: f32,
        limb_distance: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[1] = head_yaw;
        self.head.rotation[0] = head_pitch;
        self.right_ear.rotation[1] = -0.261 + head_yaw;
        self.left_ear.rotation[1] = 0.261 + head_yaw;

        let hop = (limb_angle * 0.8).cos() * 0.6 * limb_distance;
        self.left_arm.rotation[0] = -0.174 + hop;
        self.right_arm.rotation[0] = -0.174 - hop;
        self.left_thigh.rotation[0] = -0.349 - hop;
        self.right_thigh.rotation[0] = -0.349 + hop;
    }

    /// Emits the rabbit mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.left_foot.emit(root, &mut out);
        self.right_foot.emit(root, &mut out);
        self.left_thigh.emit(root, &mut out);
        self.right_thigh.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.left_arm.emit(root, &mut out);
        self.right_arm.emit(root, &mut out);
        self.head.emit(root, &mut out);
        self.right_ear.emit(root, &mut out);
        self.left_ear.emit(root, &mut out);
        self.tail.emit(root, &mut out);

        out
    }
}

impl Default for RabbitModel {
    fn default() -> Self {
        Self::new()
    }
}
