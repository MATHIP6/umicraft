// SPDX-License-Identifier: MIT OR Apache-2.0

//! Iron Golem box model matching MCP 1.8 `ModelIronGolem.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Iron Golem model (128×128 texture sheet).
#[derive(Debug, Clone)]
pub struct IronGolemModel {
    /// Head and nose.
    pub head: ModelPart,
    /// Torso and waist.
    pub body: ModelPart,
    /// Right arm (4×30×6).
    pub right_arm: ModelPart,
    /// Left arm (4×30×6).
    pub left_arm: ModelPart,
    /// Right leg (6×16×5).
    pub right_leg: ModelPart,
    /// Left leg (6×16×5).
    pub left_leg: ModelPart,
}

impl IronGolemModel {
    /// Creates a new Iron Golem model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [128.0, 128.0];
        // Ground Y=0.
        // In MCP: leg top is at Y=16, waist is Y=16..21, chest is Y=21..33, head is Y=33..43.
        let waist_py = 16.0;
        let leg_py = 13.0;
        let shoulder_py = 31.0;
        let head_py = 33.0;

        let head = ModelPart::new([0.0, head_py, -2.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -5.5],
                [8.0, 10.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, -1.0, -7.5],
                [2.0, 4.0, 2.0],
                [24.0, 0.0],
                tex,
                0.0,
            ));

        let body = ModelPart::new([0.0, waist_py, 0.0])
            // Lower waist (Y = 16..21)
            .with_box(ModelBox::new(
                [-4.5, 0.0, -3.0],
                [9.0, 5.0, 6.0],
                [0.0, 70.0],
                tex,
                0.5,
            ))
            // Upper chest (Y = 21..33)
            .with_box(ModelBox::new(
                [-9.0, 5.0, -6.0],
                [18.0, 12.0, 11.0],
                [0.0, 40.0],
                tex,
                0.0,
            ));

        let right_arm = ModelPart::new([0.0, shoulder_py, 0.0]).with_box(ModelBox::new(
            [-13.0, -27.5, -3.0],
            [4.0, 30.0, 6.0],
            [60.0, 21.0],
            tex,
            0.0,
        ));

        let left_arm = ModelPart::new([0.0, shoulder_py, 0.0]).with_box(ModelBox::new(
            [9.0, -27.5, -3.0],
            [4.0, 30.0, 6.0],
            [60.0, 58.0],
            tex,
            0.0,
        ));

        let right_leg = ModelPart::new([5.0, leg_py, 0.0]).with_box(ModelBox::new(
            [-3.5, -13.0, -3.0],
            [6.0, 16.0, 5.0],
            [60.0, 0.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([-4.0, leg_py, 0.0]).with_box(ModelBox::new(
            [-3.5, -13.0, -3.0],
            [6.0, 16.0, 5.0],
            [37.0, 0.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            right_arm,
            left_arm,
            right_leg,
            left_leg,
        }
    }

    /// Sets walking animation angles.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        let leg_angle = (limb_swing * 0.6662).cos() * 1.0 * limb_swing_amount;
        self.right_leg.rotation[0] = -leg_angle;
        self.left_leg.rotation[0] = leg_angle;

        let arm_angle = (limb_swing * 0.6662).cos() * 1.0 * limb_swing_amount;
        self.right_arm.rotation[0] = arm_angle;
        self.left_arm.rotation[0] = -arm_angle;
    }

    /// Emits all mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.right_arm.emit(root, &mut out);
        self.left_arm.emit(root, &mut out);
        self.right_leg.emit(root, &mut out);
        self.left_leg.emit(root, &mut out);
        out
    }
}

impl Default for IronGolemModel {
    fn default() -> Self {
        Self::new()
    }
}
