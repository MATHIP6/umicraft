// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wolf / Dog box model matching MCP 1.8 `ModelWolf.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Wolf model (64×32 texture sheet) with head, ears, snout, mane, body, 4 legs and tail.
#[derive(Debug, Clone)]
pub struct WolfModel {
    /// Head part with ears and snout.
    pub head: ModelPart,
    /// Torso / body.
    pub body: ModelPart,
    /// Mane / neck fur.
    pub mane: ModelPart,
    /// Back right leg (leg 1).
    pub leg1: ModelPart,
    /// Back left leg (leg 2).
    pub leg2: ModelPart,
    /// Front right leg (leg 3).
    pub leg3: ModelPart,
    /// Front left leg (leg 4).
    pub leg4: ModelPart,
    /// Tail.
    pub tail: ModelPart,
}

impl WolfModel {
    /// Creates a new Wolf model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        // In MCP: f1 = 13.5; body pivot at [0, 14, 2].
        // In Umicraft: legs 8 units high, pivot Y = 8.0.
        // Body pivot Y = 10.0. Head pivot Y = 10.5.
        let leg_py = 8.0;
        let body_py = 10.0;
        let head_py = 10.5;

        let head = ModelPart::new([-1.0, head_py, -7.0])
            // Main head cube
            .with_box(ModelBox::new(
                [-3.0, -3.0, -2.0],
                [6.0, 6.0, 4.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // Left ear
            .with_box(ModelBox::new(
                [-3.0, 3.0, 0.0],
                [2.0, 2.0, 1.0],
                [16.0, 14.0],
                tex,
                0.0,
            ))
            // Right ear
            .with_box(ModelBox::new(
                [1.0, 3.0, 0.0],
                [2.0, 2.0, 1.0],
                [16.0, 14.0],
                tex,
                0.0,
            ))
            // Snout
            .with_box(ModelBox::new(
                [-1.5, -3.0, -5.0],
                [3.0, 3.0, 4.0],
                [0.0, 10.0],
                tex,
                0.0,
            ));

        let mut body = ModelPart::new([0.0, body_py, 2.0]).with_box(ModelBox::new(
            [-3.0, -3.0, -2.0],
            [6.0, 9.0, 6.0],
            [18.0, 14.0],
            tex,
            0.0,
        ));
        body.rotation[0] = std::f32::consts::PI / 2.0;

        let mut mane = ModelPart::new([-1.0, body_py, 1.0]).with_box(ModelBox::new(
            [-3.0, -3.0, -3.0],
            [8.0, 6.0, 7.0],
            [21.0, 0.0],
            tex,
            0.0,
        ));
        mane.rotation[0] = std::f32::consts::PI / 2.0;

        let leg1 = ModelPart::new([-2.5, leg_py, 7.0]).with_box(ModelBox::new(
            [-1.0, -8.0, -1.0],
            [2.0, 8.0, 2.0],
            [0.0, 18.0],
            tex,
            0.0,
        ));

        let leg2 = ModelPart::new([0.5, leg_py, 7.0]).with_box(ModelBox::new(
            [-1.0, -8.0, -1.0],
            [2.0, 8.0, 2.0],
            [0.0, 18.0],
            tex,
            0.0,
        ));

        let leg3 = ModelPart::new([-2.5, leg_py, -4.0]).with_box(ModelBox::new(
            [-1.0, -8.0, -1.0],
            [2.0, 8.0, 2.0],
            [0.0, 18.0],
            tex,
            0.0,
        ));

        let leg4 = ModelPart::new([0.5, leg_py, -4.0]).with_box(ModelBox::new(
            [-1.0, -8.0, -1.0],
            [2.0, 8.0, 2.0],
            [0.0, 18.0],
            tex,
            0.0,
        ));

        let tail = ModelPart::new([-1.0, body_py + 2.0, 8.0]).with_box(ModelBox::new(
            [-1.0, -8.0, -1.0],
            [2.0, 8.0, 2.0],
            [9.0, 18.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            mane,
            leg1,
            leg2,
            leg3,
            leg4,
            tail,
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
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        self.leg1.rotation[0] = -leg_angle;
        self.leg2.rotation[0] = leg_angle;
        self.leg3.rotation[0] = leg_angle;
        self.leg4.rotation[0] = -leg_angle;
    }

    /// Emits all mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.mane.emit(root, &mut out);
        self.leg1.emit(root, &mut out);
        self.leg2.emit(root, &mut out);
        self.leg3.emit(root, &mut out);
        self.leg4.emit(root, &mut out);
        self.tail.emit(root, &mut out);
        out
    }
}

impl Default for WolfModel {
    fn default() -> Self {
        Self::new()
    }
}
