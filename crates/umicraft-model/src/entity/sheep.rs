// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Sheep model (MCP `ModelSheep2` body + `ModelSheep1` wool overlay).

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// 1.8 Sheep model following MCP `ModelSheep2.java` (body) and `ModelSheep1.java` (wool).
///
/// Vanilla renders the sheep in two passes with two textures:
/// - The base body (`sheep.png`) — `ModelSheep2`, a sheared-looking sheep
/// - The wool overlay (`sheep_fur.png`) — `ModelSheep1`, inflated with dilation
///
/// [`emit_body_mesh`](Self::emit_body_mesh) and [`emit_wool_mesh`](Self::emit_wool_mesh)
/// produce the vertices for each pass.
#[derive(Debug, Clone)]
pub struct SheepModel {
    // --- ModelSheep2 (base body) ---
    /// Head part.
    pub head: ModelPart,
    /// Torso part (PI/2 X rotation).
    pub body: ModelPart,
    /// Rear-right leg.
    pub leg1: ModelPart,
    /// Rear-left leg.
    pub leg2: ModelPart,
    /// Front-right leg.
    pub leg3: ModelPart,
    /// Front-left leg.
    pub leg4: ModelPart,
    // --- ModelSheep1 (wool overlay) ---
    /// Wool head overlay (dilation 0.6).
    pub wool_head: ModelPart,
    /// Wool body overlay (dilation 1.75).
    pub wool_body: ModelPart,
    /// Wool rear-right leg overlay (dilation 0.5).
    pub wool_leg1: ModelPart,
    /// Wool rear-left leg overlay (dilation 0.5).
    pub wool_leg2: ModelPart,
    /// Wool front-right leg overlay (dilation 0.5).
    pub wool_leg3: ModelPart,
    /// Wool front-left leg overlay (dilation 0.5).
    pub wool_leg4: ModelPart,
}

impl Default for SheepModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SheepModel {
    /// Creates a new sheep model with body and wool overlay.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];

        // --- ModelSheep2 (base body, uses sheep.png) ---
        // MCP: addBox(-3, -4, -6, 6, 6, 8), pivot (0, 6, -8)
        let head = ModelPart::new([0.0, 18.0, -8.0]).with_box(ModelBox::new(
            [-3.0, -2.0, -6.0],
            [6.0, 6.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        // MCP: addBox(-4, -10, -7, 8, 16, 6), pivot (0, 5, 2)
        // Rotated PI/2: local Y→world Z, local Z→world Y. Pivot formula:
        // pivot_umi_Y = 24 - pivot_mcp_Y + Z_min + Z_max = 24 - 5 + (-7) + (-1) = 11
        let mut body = ModelPart::new([0.0, 11.0, 2.0]).with_box(ModelBox::new(
            [-4.0, -10.0, -7.0],
            [8.0, 16.0, 6.0],
            [28.0, 8.0],
            tex,
            0.0,
        ));
        body.rotation[0] = PI / 2.0;

        // Legs: ModelQuadruped(12), pivot (±3, 12, ±7/±-5)
        let leg1 = ModelPart::new([-3.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg2 = ModelPart::new([3.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg3 = ModelPart::new([-3.0, 12.0, -5.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg4 = ModelPart::new([3.0, 12.0, -5.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        // --- ModelSheep1 (wool overlay, uses sheep_fur.png) ---
        // MCP: addBox(-3, -4, -4, 6, 6, 6, 0.6), pivot (0, 6, -8)
        let wool_head = ModelPart::new([0.0, 18.0, -8.0]).with_box(ModelBox::new(
            [-3.0, -2.0, -4.0],
            [6.0, 6.0, 6.0],
            [0.0, 0.0],
            tex,
            0.6,
        ));

        // MCP: addBox(-4, -10, -7, 8, 16, 6, 1.75), pivot (0, 5, 2)
        let mut wool_body = ModelPart::new([0.0, 11.0, 2.0]).with_box(ModelBox::new(
            [-4.0, -10.0, -7.0],
            [8.0, 16.0, 6.0],
            [28.0, 8.0],
            tex,
            1.75,
        ));
        wool_body.rotation[0] = PI / 2.0;

        // MCP: addBox(-2, 0, -2, 4, 6, 4, 0.5), pivot (±3, 12, ±7/±-5)
        let wool_leg1 = ModelPart::new([-3.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.5,
        ));
        let wool_leg2 = ModelPart::new([3.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.5,
        ));
        let wool_leg3 = ModelPart::new([-3.0, 12.0, -5.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.5,
        ));
        let wool_leg4 = ModelPart::new([3.0, 12.0, -5.0]).with_box(ModelBox::new(
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 4.0],
            [0.0, 16.0],
            tex,
            0.5,
        ));

        Self {
            head,
            body,
            leg1,
            leg2,
            leg3,
            leg4,
            wool_head,
            wool_body,
            wool_leg1,
            wool_leg2,
            wool_leg3,
            wool_leg4,
        }
    }

    /// Sets walking and head look angles on both body and wool parts.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        let pitch_rad = head_pitch.to_radians();
        let yaw_rad = head_yaw.to_radians();

        self.head.rotation[0] = pitch_rad;
        self.head.rotation[1] = yaw_rad;
        self.wool_head.rotation[0] = pitch_rad;
        self.wool_head.rotation[1] = yaw_rad;

        let angle1 = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        let angle2 = (limb_swing * 0.6662 + PI).cos() * 1.4 * limb_swing_amount;

        self.leg1.rotation[0] = angle1;
        self.leg2.rotation[0] = angle2;
        self.leg3.rotation[0] = angle2;
        self.leg4.rotation[0] = angle1;

        self.wool_leg1.rotation[0] = angle1;
        self.wool_leg2.rotation[0] = angle2;
        self.wool_leg3.rotation[0] = angle2;
        self.wool_leg4.rotation[0] = angle1;
    }

    /// Emits the base body mesh (for `sheep.png`).
    #[must_use]
    pub fn emit_body_mesh(&self) -> Vec<ModelVertex> {
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

    /// Emits the wool overlay mesh (for `sheep_fur.png`).
    #[must_use]
    pub fn emit_wool_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.wool_head.emit(root, &mut out);
        self.wool_body.emit(root, &mut out);
        self.wool_leg1.emit(root, &mut out);
        self.wool_leg2.emit(root, &mut out);
        self.wool_leg3.emit(root, &mut out);
        self.wool_leg4.emit(root, &mut out);
        out
    }
}
