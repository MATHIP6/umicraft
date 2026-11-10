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

    /// Turns the head (degrees) and swings the limbs while walking (MCP
    /// `ModelIronGolem.setRotationAngles` and the walking branch of
    /// `setLivingAnimations`).
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        // The golem swings each arm with the leg on the same side, on a
        // triangle wave of period 13 (MCP `ModelIronGolem`); x rotations are
        // negated for the flipped y.
        let stride = 1.5 * triangle_wave(limb_swing, 13.0) * limb_swing_amount;
        self.right_leg.rotation[0] = -stride;
        self.left_leg.rotation[0] = stride;
        self.right_arm.rotation[0] = -(-0.2 * limb_swing_amount + stride);
        self.left_arm.rotation[0] = -(-0.2 * limb_swing_amount - stride);
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

/// A triangle wave of `period` between -1 and 1: 1 at multiples of the
/// period, -1 halfway (MCP `ModelIronGolem.func_78172_a`, also the walking
/// roll of `RenderIronGolem.rotateCorpse`). Like Java's `%`, Rust's keeps the
/// sign of `x`.
///
/// ```
/// use umicraft_model::entity::triangle_wave;
/// assert_eq!(triangle_wave(0.0, 13.0), 1.0);
/// assert_eq!(triangle_wave(6.5, 13.0), -1.0);
/// ```
#[must_use]
pub fn triangle_wave(x: f32, period: f32) -> f32 {
    ((x % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

impl Default for IronGolemModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_triangle_wave_peaks_at_the_period_and_dips_halfway() {
        assert!((triangle_wave(0.0, 13.0) - 1.0).abs() < 1e-6);
        assert!((triangle_wave(6.5, 13.0) + 1.0).abs() < 1e-6);
        assert!(triangle_wave(3.25, 13.0).abs() < 1e-6);
        assert!((triangle_wave(13.0, 13.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_walking_golem_swings_each_arm_with_its_leg() {
        // Regression: the limbs followed the biped cosine (period 2pi/0.6662,
        // arms against legs) instead of the golem's triangle wave.
        let mut golem = IronGolemModel::new();
        golem.set_angles(0.0, 1.0, 0.0, 0.0);
        // MCP: right leg 1.5, right arm -0.2 + 1.5, both negated here.
        assert!((golem.right_leg.rotation[0] + 1.5).abs() < 1e-6);
        assert!((golem.right_arm.rotation[0] + 1.3).abs() < 1e-6);
        assert!((golem.left_leg.rotation[0] - 1.5).abs() < 1e-6);
        assert!((golem.left_arm.rotation[0] - 1.7).abs() < 1e-6);
    }

    #[test]
    fn a_still_golem_lets_its_arms_hang() {
        let mut golem = IronGolemModel::new();
        golem.set_angles(4.0, 0.0, 0.0, 0.0);
        assert_eq!(golem.right_arm.rotation[0], 0.0);
        assert_eq!(golem.left_leg.rotation[0], 0.0);
    }
}
