// SPDX-License-Identifier: MIT OR Apache-2.0

//! Spider and Cave Spider box models matching MCP 1.8 `ModelSpider.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Spider model (64×32 texture sheet) with 8 animated legs.
#[derive(Debug, Clone)]
pub struct SpiderModel {
    /// Head part.
    pub head: ModelPart,
    /// Neck / thorax part.
    pub neck: ModelPart,
    /// Body / abdomen part.
    pub body: ModelPart,
    /// Front right leg (leg 1).
    pub leg1: ModelPart,
    /// Front left leg (leg 2).
    pub leg2: ModelPart,
    /// Second right leg (leg 3).
    pub leg3: ModelPart,
    /// Second left leg (leg 4).
    pub leg4: ModelPart,
    /// Third right leg (leg 5).
    pub leg5: ModelPart,
    /// Third left leg (leg 6).
    pub leg6: ModelPart,
    /// Rear right leg (leg 7).
    pub leg7: ModelPart,
    /// Rear left leg (leg 8).
    pub leg8: ModelPart,
    /// Overall scale (1.0 for normal Spider, 0.7 for Cave Spider).
    pub scale: f32,
}

impl SpiderModel {
    /// Creates a standard spider model.
    #[must_use]
    pub fn new() -> Self {
        Self::with_scale(1.0)
    }

    /// Creates a spider model with a custom scale (e.g. 0.7 for Cave Spider).
    #[must_use]
    pub fn with_scale(scale: f32) -> Self {
        let tex = [64.0, 32.0];
        // In MCP: i = 15. In Umicraft (+Y up, ground Y=0, height 24):
        // pivot Y = 24 - 15 = 9.0
        let py = 9.0;

        let head = ModelPart::new([0.0, py, -3.0]).with_box(ModelBox::new(
            [-4.0, -4.0, -8.0],
            [8.0, 8.0, 8.0],
            [32.0, 4.0],
            tex,
            0.0,
        ));

        let neck = ModelPart::new([0.0, py, 0.0]).with_box(ModelBox::new(
            [-3.0, -3.0, -3.0],
            [6.0, 6.0, 6.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let body = ModelPart::new([0.0, py, 9.0]).with_box(ModelBox::new(
            [-5.0, -4.0, -6.0],
            [10.0, 8.0, 12.0],
            [0.0, 12.0],
            tex,
            0.0,
        ));

        let leg1 = ModelPart::new([-4.0, py, 2.0]).with_box(ModelBox::new(
            [-15.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg2 = ModelPart::new([4.0, py, 2.0]).with_box(ModelBox::new(
            [-1.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg3 = ModelPart::new([-4.0, py, 1.0]).with_box(ModelBox::new(
            [-15.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg4 = ModelPart::new([4.0, py, 1.0]).with_box(ModelBox::new(
            [-1.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg5 = ModelPart::new([-4.0, py, 0.0]).with_box(ModelBox::new(
            [-15.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg6 = ModelPart::new([4.0, py, 0.0]).with_box(ModelBox::new(
            [-1.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg7 = ModelPart::new([-4.0, py, -1.0]).with_box(ModelBox::new(
            [-15.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));
        let leg8 = ModelPart::new([4.0, py, -1.0]).with_box(ModelBox::new(
            [-1.0, -1.0, -1.0],
            [16.0, 2.0, 2.0],
            [18.0, 0.0],
            tex,
            0.0,
        ));

        let mut spider = Self {
            head,
            neck,
            body,
            leg1,
            leg2,
            leg3,
            leg4,
            leg5,
            leg6,
            leg7,
            leg8,
            scale,
        };
        spider.set_angles(0.0, 0.0, 0.0, 0.0);
        spider
    }

    /// Sets animation angles matching MCP `ModelSpider.setRotationAngles`.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        let pi_4 = std::f32::consts::PI / 4.0;
        // In Umicraft (+Y up): right legs (-X) need positive Z angle to slope down; left legs (+X) need negative Z angle.
        self.leg1.rotation[2] = pi_4;
        self.leg2.rotation[2] = -pi_4;
        self.leg3.rotation[2] = pi_4 * 0.74;
        self.leg4.rotation[2] = -pi_4 * 0.74;
        self.leg5.rotation[2] = pi_4 * 0.74;
        self.leg6.rotation[2] = -pi_4 * 0.74;
        self.leg7.rotation[2] = pi_4;
        self.leg8.rotation[2] = -pi_4;

        let f2 = std::f32::consts::FRAC_PI_8;
        self.leg1.rotation[1] = f2 * 2.0;
        self.leg2.rotation[1] = -f2 * 2.0;
        self.leg3.rotation[1] = f2 * 1.0;
        self.leg4.rotation[1] = -f2 * 1.0;
        self.leg5.rotation[1] = -f2 * 1.0;
        self.leg6.rotation[1] = f2 * 1.0;
        self.leg7.rotation[1] = -f2 * 2.0;
        self.leg8.rotation[1] = f2 * 2.0;

        let f3 = -((limb_swing * 0.6662 * 2.0).cos() * 0.4) * limb_swing_amount;
        let f4 =
            -(((limb_swing * 0.6662 * 2.0) + std::f32::consts::PI).cos() * 0.4) * limb_swing_amount;
        let f5 = -(((limb_swing * 0.6662 * 2.0) + (std::f32::consts::PI / 2.0)).cos() * 0.4)
            * limb_swing_amount;
        let f6 = -(((limb_swing * 0.6662 * 2.0) + (std::f32::consts::PI * 1.5)).cos() * 0.4)
            * limb_swing_amount;

        let f7 = ((limb_swing * 0.6662).sin() * 0.4).abs() * limb_swing_amount;
        let f8 =
            (((limb_swing * 0.6662) + std::f32::consts::PI).sin() * 0.4).abs() * limb_swing_amount;
        let f9 = (((limb_swing * 0.6662) + (std::f32::consts::PI / 2.0)).sin() * 0.4).abs()
            * limb_swing_amount;
        let f10 = (((limb_swing * 0.6662) + (std::f32::consts::PI * 1.5)).sin() * 0.4).abs()
            * limb_swing_amount;

        self.leg1.rotation[1] += f3;
        self.leg2.rotation[1] += -f3;
        self.leg3.rotation[1] += f4;
        self.leg4.rotation[1] += -f4;
        self.leg5.rotation[1] += f5;
        self.leg6.rotation[1] += -f5;
        self.leg7.rotation[1] += f6;
        self.leg8.rotation[1] += -f6;

        self.leg1.rotation[2] -= f7;
        self.leg2.rotation[2] += f7;
        self.leg3.rotation[2] -= f8;
        self.leg4.rotation[2] += f8;
        self.leg5.rotation[2] -= f9;
        self.leg6.rotation[2] += f9;
        self.leg7.rotation[2] -= f10;
        self.leg8.rotation[2] += f10;
    }

    /// Emits all mesh vertices scaled to blocks (1/16 block per unit * self.scale).
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(self.scale / 16.0));
        self.head.emit(root, &mut out);
        self.neck.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.leg1.emit(root, &mut out);
        self.leg2.emit(root, &mut out);
        self.leg3.emit(root, &mut out);
        self.leg4.emit(root, &mut out);
        self.leg5.emit(root, &mut out);
        self.leg6.emit(root, &mut out);
        self.leg7.emit(root, &mut out);
        self.leg8.emit(root, &mut out);
        out
    }
}

impl Default for SpiderModel {
    fn default() -> Self {
        Self::new()
    }
}
