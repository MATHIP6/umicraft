// SPDX-License-Identifier: MIT OR Apache-2.0

//! Enderman box model matching MCP 1.8 `ModelEnderman.java`.

use glam::{Mat4, Vec3};

use super::pose::{apply, vanilla_angles};
use super::{HumanoidPose, ModelBox, ModelPart, ModelVertex};

/// Height of the neck and shoulders: 30-unit legs under a 12-unit torso.
const TORSO_PIVOT_Y: f32 = 42.0;
/// How far a screaming enderman's head rises above its neck (MCP
/// `ModelEnderman`, `bipedHead.rotationPointY -= 5`).
const SCREAM_LIFT: f32 = 5.0;
/// The widest swing of an enderman limb, in radians.
const LIMB_LIMIT: f32 = 0.4;

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
        let torso_py = TORSO_PIVOT_Y;

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

    /// Poses the enderman (MCP `ModelEnderman.setRotationAngles`): the biped
    /// pose of `pose`, idle sway and attack swing included, with the arms
    /// and legs swinging half as far and at most 0.4 rad, the arms held
    /// forward while carrying a block and the head raised while screaming.
    pub fn set_pose(&mut self, pose: &HumanoidPose) {
        let mut angles = vanilla_angles(pose);
        for limb in [
            &mut angles.right_arm,
            &mut angles.left_arm,
            &mut angles.right_leg,
            &mut angles.left_leg,
        ] {
            limb[0] = (limb[0] * 0.5).clamp(-LIMB_LIMIT, LIMB_LIMIT);
        }
        if self.is_carrying {
            angles.right_arm[0] = -0.5;
            angles.left_arm[0] = -0.5;
            angles.right_arm[2] = 0.05;
            angles.left_arm[2] = -0.05;
        }

        apply(&mut self.head, [angles.head[0], angles.head[1], 0.0]);
        self.headwear.rotation = self.head.rotation;
        let head_y = if self.is_attacking {
            TORSO_PIVOT_Y + SCREAM_LIFT
        } else {
            TORSO_PIVOT_Y
        };
        self.head.pivot[1] = head_y;
        self.headwear.pivot[1] = head_y;
        // The torso turns with the attack swing but never leans forward.
        apply(&mut self.body, [0.0, angles.body[1], 0.0]);
        apply(&mut self.right_arm, angles.right_arm);
        apply(&mut self.left_arm, angles.left_arm);
        // The shoulders follow the swing sideways only.
        self.right_arm.pivot[0] = angles.right_arm_point.0;
        self.left_arm.pivot[0] = angles.left_arm_point.0;
        apply(&mut self.right_leg, angles.right_leg);
        apply(&mut self.left_leg, angles.left_leg);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn posed(carrying: bool, pose: HumanoidPose) -> EndermanModel {
        let mut model = EndermanModel::new();
        model.is_carrying = carrying;
        model.set_pose(&pose);
        model
    }

    /// Where the hand of an arm ends up, in model units.
    fn hand(arm: &ModelPart) -> Vec3 {
        arm.transform(Mat4::IDENTITY)
            .transform_point3(Vec3::new(0.0, -28.0, 0.0))
    }

    #[test]
    fn a_carrying_enderman_holds_its_arms_forward() {
        // Regression: the carrying angle kept the MCP sign, which the
        // y-up model turns around, so the arms went behind the back.
        let model = posed(true, HumanoidPose::default());
        assert!(
            hand(&model.right_arm).z < -10.0,
            "{}",
            hand(&model.right_arm)
        );
        assert!(hand(&model.left_arm).z < -10.0, "{}", hand(&model.left_arm));
    }

    #[test]
    fn an_idle_enderman_sways_its_arms_with_age() {
        // Regression: the enderman took no age, so its arms never breathed.
        let early = posed(false, HumanoidPose::default());
        let later = posed(
            false,
            HumanoidPose {
                age: 20.0,
                ..HumanoidPose::default()
            },
        );
        assert_ne!(early.right_arm.rotation, later.right_arm.rotation);
    }

    #[test]
    fn an_enderman_swings_its_arm_to_hit() {
        let rest = posed(false, HumanoidPose::default());
        let hitting = posed(
            false,
            HumanoidPose {
                swing_progress: 0.5,
                ..HumanoidPose::default()
            },
        );
        assert_ne!(rest.right_arm.rotation, hitting.right_arm.rotation);
        assert_ne!(rest.body.rotation, hitting.body.rotation);
    }

    #[test]
    fn a_running_enderman_swings_its_limbs_at_most_0_4_rad() {
        for swing in [0.0, 1.0, 2.0, 4.7] {
            let model = posed(
                false,
                HumanoidPose {
                    limb_swing: swing,
                    limb_swing_amount: 1.0,
                    ..HumanoidPose::default()
                },
            );
            for limb in [&model.right_leg, &model.left_leg] {
                assert!(limb.rotation[0].abs() <= LIMB_LIMIT + 1e-6);
            }
        }
    }

    #[test]
    fn a_screaming_enderman_lifts_its_head_and_hat() {
        let mut model = EndermanModel::new();
        model.is_attacking = true;
        model.set_pose(&HumanoidPose::default());
        assert_eq!(model.head.pivot[1], TORSO_PIVOT_Y + SCREAM_LIFT);
        assert_eq!(model.headwear.pivot[1], model.head.pivot[1]);
    }
}
