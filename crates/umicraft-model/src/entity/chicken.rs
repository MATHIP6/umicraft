// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Chicken model (MCP `ModelChicken.java`).

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::child::{emit_child, ChildLayout};
use super::{ModelBox, ModelPart, ModelVertex};

/// 1.8 Chicken model following MCP `ModelChicken.java`.
#[derive(Debug, Clone)]
pub struct ChickenModel {
    /// Head part.
    pub head: ModelPart,
    /// Beak part (rotates with head).
    pub bill: ModelPart,
    /// Wattle part (rotates with head).
    pub chin: ModelPart,
    /// Torso part (PI/2 X rotation).
    pub body: ModelPart,
    /// Right leg.
    pub right_leg: ModelPart,
    /// Left leg.
    pub left_leg: ModelPart,
    /// Right wing.
    pub right_wing: ModelPart,
    /// Left wing.
    pub left_wing: ModelPart,
}

impl Default for ChickenModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ChickenModel {
    /// Creates a chicken model (64×32 texture).
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];

        let head = ModelPart::new([0.0, 9.0, -4.0]).with_box(ModelBox::new(
            [-2.0, 0.0, -2.0],
            [4.0, 6.0, 3.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let bill = ModelPart::new([0.0, 9.0, -4.0]).with_box(ModelBox::new(
            [-2.0, 2.0, -4.0],
            [4.0, 2.0, 2.0],
            [14.0, 0.0],
            tex,
            0.0,
        ));

        let chin = ModelPart::new([0.0, 9.0, -4.0]).with_box(ModelBox::new(
            [-1.0, 0.0, -3.0],
            [2.0, 2.0, 2.0],
            [14.0, 4.0],
            tex,
            0.0,
        ));

        let mut body = ModelPart::new([0.0, 8.0, 0.0]).with_box(ModelBox::new(
            [-3.0, -4.0, -3.0],
            [6.0, 8.0, 6.0],
            [0.0, 9.0],
            tex,
            0.0,
        ));
        body.rotation[0] = PI / 2.0;

        let right_leg = ModelPart::new([-2.0, 5.0, 1.0]).with_box(ModelBox::new(
            [-1.0, -5.0, -3.0],
            [3.0, 5.0, 3.0],
            [26.0, 0.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([1.0, 5.0, 1.0]).with_box(ModelBox::new(
            [-1.0, -5.0, -3.0],
            [3.0, 5.0, 3.0],
            [26.0, 0.0],
            tex,
            0.0,
        ));

        let right_wing = ModelPart::new([-4.0, 11.0, 0.0]).with_box(ModelBox::new(
            [0.0, -4.0, -3.0],
            [1.0, 4.0, 6.0],
            [24.0, 13.0],
            tex,
            0.0,
        ));

        let left_wing = ModelPart::new([4.0, 11.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -4.0, -3.0],
            [1.0, 4.0, 6.0],
            [24.0, 13.0],
            tex,
            0.0,
        ));

        Self {
            head,
            bill,
            chin,
            body,
            right_leg,
            left_leg,
            right_wing,
            left_wing,
        }
    }

    /// Sets animation angles for walking and head look.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        let pitch_rad = head_pitch.to_radians();
        let yaw_rad = head_yaw.to_radians();

        self.head.rotation[0] = pitch_rad;
        self.head.rotation[1] = yaw_rad;
        self.bill.rotation[0] = pitch_rad;
        self.bill.rotation[1] = yaw_rad;
        self.chin.rotation[0] = pitch_rad;
        self.chin.rotation[1] = yaw_rad;

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        self.right_leg.rotation[0] = -leg_angle;
        self.left_leg.rotation[0] = leg_angle;

        // Flapping wings
        let wing_angle = (limb_swing * 0.6662).cos() * 0.4 * limb_swing_amount;
        self.right_wing.rotation[2] = wing_angle;
        self.left_wing.rotation[2] = -wing_angle;
    }

    /// Emits a chick: the head at full size, the rest halved.
    #[must_use]
    pub fn emit_child_mesh(&self) -> Vec<ModelVertex> {
        emit_child(
            &[&self.head, &self.bill, &self.chin],
            &[
                &self.body,
                &self.right_leg,
                &self.left_leg,
                &self.right_wing,
                &self.left_wing,
            ],
            ChildLayout::SMALL_ANIMAL,
        )
    }

    /// Emits all vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.bill.emit(root, &mut out);
        self.chin.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.right_leg.emit(root, &mut out);
        self.left_leg.emit(root, &mut out);
        self.right_wing.emit(root, &mut out);
        self.left_wing.emit(root, &mut out);
        out
    }
}
