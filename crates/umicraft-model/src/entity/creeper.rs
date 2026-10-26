// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Creeper model (MCP `ModelCreeper.java`).

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// 1.8 Creeper model (MCP `ModelCreeper`): head, body, and 4 short legs.
#[derive(Debug, Clone)]
pub struct CreeperModel {
    /// Head part (with outer armor layer).
    pub head: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Rear-right leg.
    pub leg1: ModelPart,
    /// Rear-left leg.
    pub leg2: ModelPart,
    /// Front-right leg.
    pub leg3: ModelPart,
    /// Front-left leg.
    pub leg4: ModelPart,
}

impl Default for CreeperModel {
    fn default() -> Self {
        Self::new()
    }
}

impl CreeperModel {
    /// Creates a creeper model (64×32 texture) following MCP `ModelCreeper.java`.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        let head = ModelPart::new([0.0, 18.0, 0.0]).with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let body = ModelPart::new([0.0, 18.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -12.0, -2.0],
            [8.0, 12.0, 4.0],
            [16.0, 16.0],
            tex,
            0.0,
        ));

        let leg1 = ModelPart::new([-2.0, 6.0, 4.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg2 = ModelPart::new([2.0, 6.0, 4.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg3 = ModelPart::new([-2.0, 6.0, -4.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg4 = ModelPart::new([2.0, 6.0, -4.0]).with_box(ModelBox::new(
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

    /// Sets walking and head-look angles matching MCP `ModelCreeper.setRotationAngles`.
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
