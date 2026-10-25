// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Humanoid biped model (Player, Zombie, Skeleton).

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// 1.8 Humanoid model (Player, Zombie, Skeleton) supporting Steve and Alex arm widths.
#[derive(Debug, Clone)]
pub struct HumanoidModel {
    /// Head part.
    pub head: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Right arm part.
    pub right_arm: ModelPart,
    /// Left arm part.
    pub left_arm: ModelPart,
    /// Right leg part.
    pub right_leg: ModelPart,
    /// Left leg part.
    pub left_leg: ModelPart,
}

impl HumanoidModel {
    /// Builds a 1.8 player humanoid model (Steve or Alex) with base and outer layers.
    #[must_use]
    pub fn new(is_slim: bool) -> Self {
        Self::create(is_slim, false)
    }

    /// Builds a 1.8 zombie biped model with mirrored 64×32 limb textures.
    #[must_use]
    pub fn zombie() -> Self {
        Self::create(false, true)
    }

    /// Builds a 1.8 skeleton biped with thin 2-pixel-wide limbs (MCP `ModelSkeleton`).
    #[must_use]
    pub fn skeleton() -> Self {
        let tex = [64.0, 32.0];

        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [32.0, 0.0],
                tex,
                0.5,
            ));

        let body = ModelPart::new([0.0, 12.0, 0.0]).with_box(ModelBox::new(
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            [16.0, 16.0],
            tex,
            0.0,
        ));

        // Thin 2px arms
        let right_arm = ModelPart::new([-5.0, 22.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -1.0],
            [2.0, 12.0, 2.0],
            [40.0, 16.0],
            tex,
            0.0,
        ));

        let left_arm = ModelPart::new([5.0, 22.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -1.0],
            [2.0, 12.0, 2.0],
            [40.0, 16.0],
            tex,
            0.0,
        ));

        // Thin 2px legs
        let right_leg = ModelPart::new([-2.0, 12.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -12.0, -1.0],
            [2.0, 12.0, 2.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([2.0, 12.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -12.0, -1.0],
            [2.0, 12.0, 2.0],
            [0.0, 16.0],
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

    fn create(is_slim: bool, is_mob: bool) -> Self {
        let tex = [64.0, 64.0];

        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [32.0, 0.0],
                tex,
                0.5,
            ));

        let body = ModelPart::new([0.0, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -2.0],
                [8.0, 12.0, 4.0],
                [16.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -2.0],
                [8.0, 12.0, 4.0],
                [16.0, 32.0],
                tex,
                0.25,
            ));

        let arm_w = if is_slim { 3.0 } else { 4.0 };
        let right_arm = ModelPart::new([-5.0, 22.0, 0.0])
            .with_box(ModelBox::new(
                [-arm_w + 1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                [40.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-arm_w + 1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                [40.0, 32.0],
                tex,
                0.25,
            ));

        let left_arm_uv = if is_mob { [40.0, 16.0] } else { [32.0, 48.0] };
        let left_arm_wear_uv = if is_mob { [40.0, 16.0] } else { [48.0, 48.0] };
        let left_arm = ModelPart::new([5.0, 22.0, 0.0])
            .with_box(ModelBox::new(
                [-1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                left_arm_uv,
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                left_arm_wear_uv,
                tex,
                0.25,
            ));

        let right_leg = ModelPart::new([-1.9, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                [0.0, 32.0],
                tex,
                0.25,
            ));

        let left_leg_uv = if is_mob { [0.0, 16.0] } else { [16.0, 48.0] };
        let left_leg_wear_uv = if is_mob { [0.0, 16.0] } else { [0.0, 48.0] };
        let left_leg = ModelPart::new([1.9, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                left_leg_uv,
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                left_leg_wear_uv,
                tex,
                0.25,
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

    /// Sets animation angles based on limb swing and head orientation.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
        is_zombie: bool,
    ) {
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        self.right_leg.rotation[0] = leg_angle;
        self.left_leg.rotation[0] = -leg_angle;

        if is_zombie {
            self.right_arm.rotation[0] = PI / 2.0;
            self.left_arm.rotation[0] = PI / 2.0;
            self.right_arm.rotation[1] = -0.1;
            self.left_arm.rotation[1] = 0.1;
            self.right_arm.rotation[2] = 0.0;
            self.left_arm.rotation[2] = 0.0;
        } else {
            let arm_angle = (limb_swing * 0.6662).cos() * 2.0 * limb_swing_amount * 0.5;
            self.right_arm.rotation[0] = -arm_angle;
            self.left_arm.rotation[0] = arm_angle;
        }
    }

    /// Applies the vanilla 1.8 sneaking crouch pose (MCP `ModelBiped.setRotationAngles`).
    ///
    /// After calling this, the caller must also shift the world-space render
    /// position up by 0.2 blocks to match the vanilla `GlStateManager.translate`
    /// that prevents ground clipping.
    pub fn apply_sneaking(&mut self) {
        self.body.rotation[0] = 0.5;
        self.right_arm.rotation[0] += 0.4;
        self.left_arm.rotation[0] += 0.4;
        self.right_leg.pivot[2] = 4.0;
        self.left_leg.pivot[2] = 4.0;
        self.right_leg.pivot[1] = 15.0;
        self.left_leg.pivot[1] = 15.0;
        self.head.pivot[1] = 23.0;
    }

    /// Generates the complete vertex mesh in blocks (1 unit = 1/16 block).
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
