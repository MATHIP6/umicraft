// SPDX-License-Identifier: MIT OR Apache-2.0

//! Humanoid posing: a port of MCP `ModelBiped.setRotationAngles` and
//! `ModelZombie.setRotationAngles`.
//!
//! The maths is written with the vanilla angles. The models of this crate are
//! the vanilla ones mirrored along y (y up, pivots at `24 - y`), which turns
//! a vanilla rotation `(x, y, z)` into `(-x, y, -z)`; [`HumanoidModel::set_pose`]
//! applies that conversion in one place.

use std::f32::consts::PI;

use super::humanoid::HumanoidModel;
use super::ModelPart;

/// What the right hand of a humanoid is doing
/// (MCP `ModelBiped.heldItemRight` and `aimedBow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArmPose {
    /// Nothing in the hand: the arm swings freely.
    #[default]
    Empty,
    /// An item in the hand: the arm is held a little forward.
    Holding,
    /// A sword held up to block (`heldItemRight == 3`).
    Blocking,
    /// A bow being drawn: both arms point where the head looks.
    AimingBow,
}

/// The inputs of one humanoid pose, in the units of the vanilla model.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HumanoidPose {
    /// Limb swing phase (MCP `limbSwing`).
    pub limb_swing: f32,
    /// Limb swing amplitude, `0..=1`.
    pub limb_swing_amount: f32,
    /// Ticks lived plus the partial tick; drives the idle arm sway.
    pub age: f32,
    /// Head yaw relative to the body, in degrees.
    pub head_yaw: f32,
    /// Head pitch in degrees, positive looking down.
    pub head_pitch: f32,
    /// Arm swing progress `0..=1` (MCP `swingProgress`).
    pub swing_progress: f32,
    /// Crouching (players only in 1.8).
    pub sneaking: bool,
    /// The right hand.
    pub arms: ArmPose,
    /// Arms stretched forward, as zombies and skeletons hold them
    /// (MCP `ModelZombie`).
    pub arms_forward: bool,
}

/// Vanilla rotations of the six parts, `[x, y, z]` in radians.
struct Angles {
    head: [f32; 2],
    body: [f32; 2],
    right_arm: [f32; 3],
    left_arm: [f32; 3],
    right_leg: [f32; 3],
    left_leg: [f32; 3],
    /// Pivot offsets of the arms, `(x, z)`.
    right_arm_point: (f32, f32),
    left_arm_point: (f32, f32),
}

impl HumanoidModel {
    /// Poses the model (MCP `ModelBiped.setRotationAngles`, then
    /// `ModelZombie.setRotationAngles` when the arms are held forward).
    pub fn set_pose(&mut self, pose: &HumanoidPose) {
        let angles = vanilla_angles(pose);
        apply(&mut self.head, [angles.head[0], angles.head[1], 0.0]);
        self.head.pivot[1] = 24.0 - if pose.sneaking { 1.0 } else { 0.0 };
        apply(&mut self.body, [angles.body[0], angles.body[1], 0.0]);
        apply(&mut self.right_arm, angles.right_arm);
        apply(&mut self.left_arm, angles.left_arm);
        self.right_arm.pivot[0] = angles.right_arm_point.0;
        self.right_arm.pivot[2] = angles.right_arm_point.1;
        self.left_arm.pivot[0] = angles.left_arm_point.0;
        self.left_arm.pivot[2] = angles.left_arm_point.1;
        apply(&mut self.right_leg, angles.right_leg);
        apply(&mut self.left_leg, angles.left_leg);
        let (leg_y, leg_z) = if pose.sneaking {
            (24.0 - 9.0, 4.0)
        } else {
            (24.0 - 12.0, 0.1)
        };
        for leg in [&mut self.right_leg, &mut self.left_leg] {
            leg.pivot[1] = leg_y;
            leg.pivot[2] = leg_z;
        }
    }
}

/// Sets a part's rotation from vanilla angles.
fn apply(part: &mut ModelPart, vanilla: [f32; 3]) {
    part.rotation = [-vanilla[0], vanilla[1], -vanilla[2]];
}

fn vanilla_angles(pose: &HumanoidPose) -> Angles {
    let swing = pose.limb_swing;
    let amount = pose.limb_swing_amount;
    let head_y = pose.head_yaw.to_radians();
    let head_x = pose.head_pitch.to_radians();

    let mut right_arm = [(swing * 0.6662 + PI).cos() * 2.0 * amount * 0.5, 0.0, 0.0];
    let mut left_arm = [(swing * 0.6662).cos() * 2.0 * amount * 0.5, 0.0, 0.0];
    let right_leg = [(swing * 0.6662).cos() * 1.4 * amount, 0.0, 0.0];
    let left_leg = [(swing * 0.6662 + PI).cos() * 1.4 * amount, 0.0, 0.0];

    match pose.arms {
        ArmPose::Holding => right_arm[0] = right_arm[0] * 0.5 - PI / 10.0,
        ArmPose::Blocking => {
            right_arm[0] = right_arm[0] * 0.5 - PI / 10.0 * 3.0;
            right_arm[1] = -PI / 6.0;
        }
        ArmPose::Empty | ArmPose::AimingBow => {}
    }

    // The swing of the attacking arm turns the torso and moves the shoulders.
    let progress = pose.swing_progress;
    let body_y = (progress.sqrt() * PI * 2.0).sin() * 0.2;
    let right_arm_point = (-body_y.cos() * 5.0, body_y.sin() * 5.0);
    let left_arm_point = (body_y.cos() * 5.0, -body_y.sin() * 5.0);
    right_arm[1] += body_y;
    left_arm[1] += body_y;
    left_arm[0] += body_y;
    let ease = 1.0 - (1.0 - progress).powi(4);
    let lift = (ease * PI).sin();
    let reach = (progress * PI).sin() * -(head_x - 0.7) * 0.75;
    right_arm[0] -= lift * 1.2 + reach;
    right_arm[1] += body_y * 2.0;
    right_arm[2] += (progress * PI).sin() * -0.4;

    let mut body_x = 0.0;
    if pose.sneaking {
        body_x = 0.5;
        right_arm[0] += 0.4;
        left_arm[0] += 0.4;
    }

    if pose.arms_forward {
        // `ModelZombie` replaces the arm angles after `ModelBiped`.
        let reach = (progress * PI).sin();
        let ease = (1.0 - (1.0 - progress) * (1.0 - progress)) * PI;
        let forward = -PI / 2.0 - (reach * 1.2 - ease.sin() * 0.4);
        right_arm = [forward, -(0.1 - reach * 0.6), 0.0];
        left_arm = [forward, 0.1 - reach * 0.6, 0.0];
    }

    sway(&mut right_arm, &mut left_arm, pose.age);

    if pose.arms == ArmPose::AimingBow {
        right_arm[2] = 0.0;
        left_arm[2] = 0.0;
        right_arm[1] = -0.1 + head_y;
        left_arm[1] = 0.1 + head_y + 0.4;
        right_arm[0] = -PI / 2.0 + head_x;
        left_arm[0] = -PI / 2.0 + head_x;
        sway(&mut right_arm, &mut left_arm, pose.age);
    }

    Angles {
        head: [head_x, head_y],
        body: [body_x, body_y],
        right_arm,
        left_arm,
        right_leg,
        left_leg,
        right_arm_point,
        left_arm_point,
    }
}

/// The slow breathing sway every humanoid arm has.
fn sway(right_arm: &mut [f32; 3], left_arm: &mut [f32; 3], age: f32) {
    let roll = (age * 0.09).cos() * 0.05 + 0.05;
    let pitch = (age * 0.067).sin() * 0.05;
    right_arm[2] += roll;
    left_arm[2] -= roll;
    right_arm[0] += pitch;
    left_arm[0] -= pitch;
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Vec3};
    use std::f32::consts::PI;

    fn posed(pose: HumanoidPose) -> HumanoidModel {
        let mut model = HumanoidModel::new(false);
        model.set_pose(&pose);
        model
    }

    /// Where the bottom of a limb ends up, in model units.
    fn tip(part: &ModelPart, local: Vec3) -> Vec3 {
        part.transform(Mat4::IDENTITY).transform_point3(local)
    }

    #[test]
    fn a_standing_player_hangs_straight_with_a_slight_sway() {
        let model = posed(HumanoidPose::default());
        assert!(model.right_leg.rotation[0].abs() < 1e-6);
        assert!(model.body.rotation[0].abs() < 1e-6);
        // Only the breathing roll moves the arms.
        assert!((model.right_arm.rotation[2] + 0.1).abs() < 1e-5);
        assert!((model.left_arm.rotation[2] - 0.1).abs() < 1e-5);
    }

    #[test]
    fn looking_down_tips_the_face_down() {
        let model = posed(HumanoidPose {
            head_pitch: 45.0,
            ..HumanoidPose::default()
        });
        // The front of the head is at -z; a positive pitch lowers it.
        let nose = tip(&model.head, Vec3::new(0.0, 4.0, -4.0));
        assert!(nose.y < 24.0 + 4.0, "{nose}");
    }

    #[test]
    fn the_right_arm_and_the_left_leg_swing_together() {
        let model = posed(HumanoidPose {
            limb_swing: 1.0,
            limb_swing_amount: 1.0,
            ..HumanoidPose::default()
        });
        let hand = tip(&model.right_arm, Vec3::new(0.0, -10.0, 0.0)).z;
        let foot = tip(&model.left_leg, Vec3::new(0.0, -12.0, 0.0)).z;
        assert!(hand * foot > 0.0, "same side: {hand} {foot}");
        let other_foot = tip(&model.right_leg, Vec3::new(0.0, -12.0, 0.0)).z;
        assert!(hand * other_foot < 0.0);
    }

    #[test]
    fn sneaking_leans_the_torso_forward_and_lifts_the_hips() {
        let model = posed(HumanoidPose {
            sneaking: true,
            ..HumanoidPose::default()
        });
        // The shoulders move forward (-z) and the hips rise by three pixels.
        let shoulder = tip(&model.body, Vec3::new(0.0, 0.0, 0.0));
        let waist = tip(&model.body, Vec3::new(0.0, -12.0, 0.0));
        assert!(waist.z > shoulder.z, "the torso leans forward");
        assert_eq!(model.right_leg.pivot[1], 15.0);
        assert_eq!(model.right_leg.pivot[2], 4.0);
        assert_eq!(model.head.pivot[1], 23.0);
    }

    #[test]
    fn the_arm_swing_turns_the_torso_and_raises_the_arm() {
        let rest = posed(HumanoidPose::default());
        let swung = posed(HumanoidPose {
            swing_progress: 0.15,
            ..HumanoidPose::default()
        });
        assert!(swung.body.rotation[1].abs() > 0.1);
        let before = tip(&rest.right_arm, Vec3::new(0.0, -10.0, 0.0)).z;
        let after = tip(&swung.right_arm, Vec3::new(0.0, -10.0, 0.0)).z;
        assert!(
            after < before - 3.0,
            "the hand comes forward: {before} {after}"
        );
    }

    #[test]
    fn a_finished_swing_leaves_the_pose_untouched() {
        let rest = posed(HumanoidPose::default());
        let done = posed(HumanoidPose {
            swing_progress: 1.0,
            ..HumanoidPose::default()
        });
        assert!((rest.right_arm.rotation[0] - done.right_arm.rotation[0]).abs() < 1e-5);
        assert!(done.body.rotation[1].abs() < 1e-5);
    }

    #[test]
    fn zombies_hold_both_arms_forward() {
        let model = posed(HumanoidPose {
            arms_forward: true,
            ..HumanoidPose::default()
        });
        for arm in [&model.right_arm, &model.left_arm] {
            assert!(tip(arm, Vec3::new(0.0, -10.0, 0.0)).z < -8.0);
        }
    }

    #[test]
    fn a_blocking_sword_arm_is_held_across_the_body() {
        let model = posed(HumanoidPose {
            arms: ArmPose::Blocking,
            ..HumanoidPose::default()
        });
        assert!((model.right_arm.rotation[1] + PI / 6.0).abs() < 1e-5);
        let hand = tip(&model.right_arm, Vec3::new(0.0, -10.0, 0.0));
        assert!(hand.z < -2.0, "raised forward: {hand}");
    }

    #[test]
    fn a_drawn_bow_points_both_arms_at_the_target() {
        let model = posed(HumanoidPose {
            arms: ArmPose::AimingBow,
            head_pitch: 20.0,
            ..HumanoidPose::default()
        });
        for arm in [&model.right_arm, &model.left_arm] {
            assert!(tip(arm, Vec3::new(0.0, -10.0, 0.0)).z < -8.0);
        }
    }
}
