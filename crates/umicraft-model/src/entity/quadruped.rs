// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Quadruped model (Cow, Pig).

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Quadruped model (Cow, Pig).
#[derive(Debug, Clone)]
pub struct QuadrupedModel {
    /// Head part.
    pub head: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Leg 1 (rear right).
    pub leg1: ModelPart,
    /// Leg 2 (rear left).
    pub leg2: ModelPart,
    /// Leg 3 (front right).
    pub leg3: ModelPart,
    /// Leg 4 (front left).
    pub leg4: ModelPart,
}

impl QuadrupedModel {
    /// Creates a cow model (64×32 texture) exactly following MCP `ModelCow.java` and `ModelQuadruped.java`.
    #[must_use]
    pub fn cow() -> Self {
        let tex = [64.0, 32.0];
        let head = ModelPart::new([0.0, 20.0, -8.0])
            .with_box(ModelBox::new(
                [-4.0, -4.0, -6.0],
                [8.0, 8.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.0, 2.0, -4.0],
                [1.0, 3.0, 1.0],
                [22.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [4.0, 2.0, -4.0],
                [1.0, 3.0, 1.0],
                [22.0, 0.0],
                tex,
                0.0,
            ));

        // Body: pivot at [0, 5, 2] in MCP (Y=15 in Umicraft) with rotation +PI/2
        let mut body = ModelPart::new([0.0, 15.0, 2.0])
            .with_box(ModelBox::new(
                [-6.0, -10.0, -7.0],
                [12.0, 18.0, 10.0],
                [18.0, 4.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, 2.0, 3.0],
                [4.0, 6.0, 1.0],
                [52.0, 0.0],
                tex,
                0.0,
            ));
        body.rotation[0] = PI / 2.0;

        let leg1 = ModelPart::new([-4.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg2 = ModelPart::new([4.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg3 = ModelPart::new([-4.0, 12.0, -6.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg4 = ModelPart::new([4.0, 12.0, -6.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            leg1,
            leg2,
            leg3,
            leg4,
        }
    }

    /// Creates a pig model (64×32 texture) following MCP `ModelPig.java` / `ModelQuadruped.java`.
    #[must_use]
    pub fn pig() -> Self {
        let tex = [64.0, 32.0];

        let head = ModelPart::new([0.0, 12.0, -6.0])
            .with_box(ModelBox::new(
                [-4.0, -4.0, -8.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -3.0, -9.0],
                [4.0, 3.0, 1.0],
                [16.0, 16.0],
                tex,
                0.0,
            ));

        let mut body = ModelPart::new([0.0, 7.0, 2.0]).with_box(ModelBox::new(
            [-5.0, -10.0, -7.0],
            [10.0, 16.0, 8.0],
            [28.0, 8.0],
            tex,
            0.0,
        ));
        body.rotation[0] = PI / 2.0;

        let leg1 = ModelPart::new([-3.0, 6.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg2 = ModelPart::new([3.0, 6.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg3 = ModelPart::new([-3.0, 6.0, -5.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg4 = ModelPart::new([3.0, 6.0, -5.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            leg1,
            leg2,
            leg3,
            leg4,
        }
    }

    /// Sets walking animation angles matching MCP `ModelQuadruped.java`.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        self.leg1.rotation[0] = -leg_angle;
        self.leg2.rotation[0] = leg_angle;
        self.leg3.rotation[0] = leg_angle;
        self.leg4.rotation[0] = -leg_angle;
    }

    /// Emits all vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.leg1.emit(root, &mut out);
        self.leg2.emit(root, &mut out);
        self.leg3.emit(root, &mut out);
        self.leg4.emit(root, &mut out);
        out
    }
}
