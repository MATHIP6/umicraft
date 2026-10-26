// SPDX-License-Identifier: MIT OR Apache-2.0

//! Enderman box model matching MCP 1.8 `ModelEnderman.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Enderman model (64×32 texture sheet) with long, slender limbs.
#[derive(Debug, Clone)]
pub struct EndermanModel {
    /// Head part.
    pub head: ModelPart,
    /// Headwear outer layer.
    pub headwear: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Right arm (2×30×2).
    pub right_arm: ModelPart,
    /// Left arm (2×30×2).
    pub left_arm: ModelPart,
    /// Right leg (2×30×2).
    pub right_leg: ModelPart,
    /// Left leg (2×30×2).
    pub left_leg: ModelPart,
    /// Whether the enderman is carrying a block.
    pub is_carrying: bool,
    /// Whether the enderman is screaming/attacking.
    pub is_attacking: bool,
}

impl EndermanModel {
    /// Creates a new Enderman model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        // In MCP, f = -14.0. In Umicraft:
        // Ground is Y=0. Total height ~3 blocks (46 units).
        // Legs are 30 units long: pivot Y = 30.0.
        // Body is 12 units high: pivot Y = 30.0 + 12.0 = 42.0.
        // Head pivot Y = 42.0.
        let leg_py = 30.0;
        let torso_py = 42.0;

        let head = ModelPart::new([0.0, torso_py, 0.0]).with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let headwear = ModelPart::new([0.0, torso_py, 0.0]).with_box(ModelBox::new(
            [-3.5, 0.0, -3.5],
            [7.0, 8.0, 7.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        let body = ModelPart::new([0.0, torso_py, 0.0]).with_box(ModelBox::new(
            [-4.0, -12.0, -2.0],
            [8.0, 12.0, 4.0],
            [32.0, 16.0],
            tex,
            0.0,
        ));

        let right_arm = ModelPart::new([-5.0, torso_py - 2.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -28.0, -1.0],
            [2.0, 30.0, 2.0],
            [56.0, 0.0],
            tex,
            0.0,
        ));

        let left_arm = ModelPart::new([5.0, torso_py - 2.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -28.0, -1.0],
            [2.0, 30.0, 2.0],
            [56.0, 0.0],
            tex,
            0.0,
        ));

        let right_leg = ModelPart::new([-2.0, leg_py, 0.0]).with_box(ModelBox::new(
            [-1.0, -30.0, -1.0],
            [2.0, 30.0, 2.0],
            [56.0, 0.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([2.0, leg_py, 0.0]).with_box(ModelBox::new(
            [-1.0, -30.0, -1.0],
            [2.0, 30.0, 2.0],
            [56.0, 0.0],
            tex,
            0.0,
        ));

        Self {
            head,
            headwear,
            body,
            right_arm,
            left_arm,
            right_leg,
            left_leg,
            is_carrying: false,
            is_attacking: false,
        }
    }

    /// Poses the Enderman according to walking speed and looking directions.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();
        self.headwear.rotation = self.head.rotation;

        if self.is_attacking {
            self.head.pivot[1] = 47.0;
        } else {
            self.head.pivot[1] = 42.0;
        }

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount * 0.5;
        let leg_angle_clamped = leg_angle.clamp(-0.4, 0.4);
        self.right_leg.rotation[0] = leg_angle_clamped;
        self.left_leg.rotation[0] = -leg_angle_clamped;

        if self.is_carrying {
            self.right_arm.rotation[0] = -0.5;
            self.left_arm.rotation[0] = -0.5;
            self.right_arm.rotation[2] = 0.05;
            self.left_arm.rotation[2] = -0.05;
        } else {
            let arm_angle = (limb_swing * 0.6662).cos() * 2.0 * limb_swing_amount * 0.25;
            let arm_angle_clamped = arm_angle.clamp(-0.4, 0.4);
            self.right_arm.rotation[0] = -arm_angle_clamped;
            self.left_arm.rotation[0] = arm_angle_clamped;
        }
    }

    /// Emits the mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.headwear.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.right_arm.emit(root, &mut out);
        self.left_arm.emit(root, &mut out);
        self.right_leg.emit(root, &mut out);
        self.left_leg.emit(root, &mut out);
        out
    }
}

impl Default for EndermanModel {
    fn default() -> Self {
        Self::new()
    }
}
