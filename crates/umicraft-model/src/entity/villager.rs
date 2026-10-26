// SPDX-License-Identifier: MIT OR Apache-2.0

//! Villager, Zombie Villager and Witch box models matching MCP 1.8 `ModelVillager.java` and `ModelWitch.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Villager and Witch model with distinctive large nose, long robe and folded arms.
#[derive(Debug, Clone)]
pub struct VillagerModel {
    /// Head part with large nose.
    pub head: ModelPart,
    /// Torso part with outer robe layer.
    pub body: ModelPart,
    /// Crossed arms part.
    pub arms: ModelPart,
    /// Right leg.
    pub right_leg: ModelPart,
    /// Left leg.
    pub left_leg: ModelPart,
    /// Witch hat (present for Witch).
    pub witch_hat: Option<ModelPart>,
}

impl VillagerModel {
    /// Creates a standard villager model (64×64 texture sheet).
    #[must_use]
    pub fn villager() -> Self {
        Self::create(false)
    }

    /// Creates a witch model (64×128 texture sheet with hat).
    #[must_use]
    pub fn witch() -> Self {
        Self::create(true)
    }

    fn create(is_witch: bool) -> Self {
        let tex = if is_witch {
            [64.0, 128.0]
        } else {
            [64.0, 64.0]
        };

        // Ground is Y=0.
        // Legs: 12 units high, pivot at [±2, 12, 0].
        // Body: 12 units high, torso pivot at [0, 24, 0].
        // Head: pivot at [0, 24, 0].
        let torso_py = 24.0;
        let leg_py = 12.0;

        let mut head = ModelPart::new([0.0, torso_py, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 10.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // Nose: 2×4×2 extending forward
            .with_box(ModelBox::new(
                [-1.0, 0.0, -6.0],
                [2.0, 4.0, 2.0],
                [24.0, 0.0],
                tex,
                0.0,
            ));

        if is_witch {
            // Mole on nose
            head = head.with_box(ModelBox::new(
                [0.0, 1.0, -6.75],
                [1.0, 1.0, 1.0],
                [0.0, 0.0],
                tex,
                -0.25,
            ));
        }

        // Body: base torso (8×12×6) + outer coat/robe (8×18×6)
        let body = ModelPart::new([0.0, torso_py, 0.0])
            .with_box(ModelBox::new(
                [-4.0, -12.0, -3.0],
                [8.0, 12.0, 6.0],
                [16.0, 20.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -18.0, -3.0],
                [8.0, 18.0, 6.0],
                [0.0, 38.0],
                tex,
                0.5,
            ));

        // Folded arms: pivot at [0, 21, -1] rotated by +0.75 rad (tilts forward in front of torso)
        let mut arms = ModelPart::new([0.0, torso_py - 3.0, -1.0])
            // Left arm
            .with_box(ModelBox::new(
                [-8.0, -6.0, -2.0],
                [4.0, 8.0, 4.0],
                [44.0, 22.0],
                tex,
                0.0,
            ))
            // Right arm
            .with_box(ModelBox::new(
                [4.0, -6.0, -2.0],
                [4.0, 8.0, 4.0],
                [44.0, 22.0],
                tex,
                0.0,
            ))
            // Middle crossed segment (at the bottom of vertical sleeves)
            .with_box(ModelBox::new(
                [-4.0, -6.0, -2.0],
                [8.0, 4.0, 4.0],
                [40.0, 38.0],
                tex,
                0.0,
            ));
        arms.rotation[0] = 0.75;

        let right_leg = ModelPart::new([-2.0, leg_py, 0.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 22.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([2.0, leg_py, 0.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 22.0],
            tex,
            0.0,
        ));

        let witch_hat = if is_witch {
            let hat = ModelPart::new([0.0, torso_py + 10.0, 0.0])
                // Brim
                .with_box(ModelBox::new(
                    [-5.0, 0.0, -5.0],
                    [10.0, 2.0, 10.0],
                    [0.0, 64.0],
                    tex,
                    0.0,
                ))
                // Middle cone
                .with_box(ModelBox::new(
                    [-3.5, 2.0, -3.5],
                    [7.0, 4.0, 7.0],
                    [0.0, 76.0],
                    tex,
                    0.0,
                ))
                // Upper cone
                .with_box(ModelBox::new(
                    [-2.0, 6.0, -2.0],
                    [4.0, 4.0, 4.0],
                    [0.0, 87.0],
                    tex,
                    0.0,
                ))
                // Tip
                .with_box(ModelBox::new(
                    [-0.5, 10.0, -0.5],
                    [1.0, 2.0, 1.0],
                    [0.0, 95.0],
                    tex,
                    0.25,
                ));
            Some(hat)
        } else {
            None
        };

        Self {
            head,
            body,
            arms,
            right_leg,
            left_leg,
            witch_hat,
        }
    }

    /// Sets walking and looking angles matching MCP `ModelVillager.setRotationAngles`.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();
        if let Some(hat) = self.witch_hat.as_mut() {
            hat.rotation = self.head.rotation;
        }

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount * 0.5;
        self.right_leg.rotation[0] = leg_angle;
        self.left_leg.rotation[0] = -leg_angle;
    }

    /// Emits all mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.arms.emit(root, &mut out);
        self.right_leg.emit(root, &mut out);
        self.left_leg.emit(root, &mut out);
        if let Some(hat) = self.witch_hat.as_ref() {
            hat.emit(root, &mut out);
        }
        out
    }
}
