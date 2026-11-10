// SPDX-License-Identifier: MIT OR Apache-2.0

//! Snowman (Snow Golem) box model matching MCP 1.8 `ModelSnowMan.java`.

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Height of the shoulders (vanilla y 6).
const HAND_PIVOT_Y: f32 = 18.0;
/// How far from the body's axis the arms are set, in pixels.
const SHOULDER: f32 = 5.0;

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

        // MCP: both hands are the same stick from the shoulder at y 6
        // (here 18), the left one turned half round; `set_angles` places them.
        let hand = || {
            ModelPart::new([0.0, HAND_PIVOT_Y, 0.0]).with_box(ModelBox::new(
                [-1.0, -2.0, -1.0],
                [12.0, 2.0, 2.0],
                [32.0, 0.0],
                tex,
                -0.5,
            ))
        };
        let (right_hand, left_hand) = (hand(), hand());

        let mut snowman = Self {
            head,
            body,
            bottom_body,
            right_hand,
            left_hand,
        };
        snowman.set_angles(0.0, 0.0);
        snowman
    }

    /// Turns the head (degrees) and the upper body by a quarter of it, and
    /// carries the stick arms round with the body (MCP
    /// `ModelSnowMan.setRotationAngles`; vanilla x and z angles negated for
    /// the flipped y).
    pub fn set_angles(&mut self, head_yaw: f32, head_pitch: f32) {
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();
        let body_yaw = head_yaw.to_radians() * 0.25;
        self.body.rotation[1] = body_yaw;
        let (sin, cos) = body_yaw.sin_cos();
        self.right_hand.pivot = [cos * SHOULDER, HAND_PIVOT_Y, -sin * SHOULDER];
        self.left_hand.pivot = [-cos * SHOULDER, HAND_PIVOT_Y, sin * SHOULDER];
        self.right_hand.rotation = [0.0, body_yaw, -1.0];
        self.left_hand.rotation = [0.0, PI + body_yaw, 1.0];
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the tip of a stick arm ends up, in model units.
    fn tip(hand: &ModelPart) -> Vec3 {
        hand.transform(Mat4::IDENTITY)
            .transform_point3(Vec3::new(11.0, -1.0, 0.0))
    }

    #[test]
    fn the_arms_turn_with_the_body() {
        // Regression: the arms stayed put while the upper body turned.
        let mut snowman = SnowmanModel::new();
        let (right, left) = (tip(&snowman.right_hand), tip(&snowman.left_hand));
        assert!(right.x > 5.0 && left.x < -5.0, "{right} {left}");
        snowman.set_angles(180.0, 0.0);
        let turned = tip(&snowman.right_hand);
        assert!(turned.z.abs() > 3.0, "the right arm swings round: {turned}");
        assert!((snowman.right_hand.pivot[2] + SHOULDER * (PI / 4.0).sin()).abs() < 1e-5);
    }
}
