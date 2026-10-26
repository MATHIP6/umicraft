// SPDX-License-Identifier: MIT OR Apache-2.0

//! Villager and Witch box model matching MCP 1.8 `ModelVillager.java` and `ModelWitch.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Villager and Witch model (64×64 or 64×128 texture sheet) with robe, folded arms and elongated head.
#[derive(Debug, Clone)]
pub struct VillagerModel {
    /// Head part with elongated forehead and protruding nose.
    pub head: ModelPart,
    /// Torso part with robe body.
    pub body: ModelPart,
    /// Folded arms held in front of chest.
    pub arms: ModelPart,
    /// Right leg.
    pub right_leg: ModelPart,
    /// Left leg.
    pub left_leg: ModelPart,
    /// Hierarchical witch hat sections (brim, lower cone, upper cone, crooked tip).
    pub witch_hat: Option<Vec<ModelPart>>,
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
            // Mole on nose (MCP: 1x1x1 at z = -6.75)
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
            // MCP ModelWitch hierarchy:
            // 1. witchHat attached to villagerHead at (0, 10, 0), box 10×2×10
            let brim = ModelPart::new([0.0, 10.0, 0.0]).with_box(ModelBox::new(
                [-5.0, 0.0, -5.0],
                [10.0, 2.0, 10.0],
                [0.0, 64.0],
                tex,
                0.0,
            ));

            // 2. Middle cone child at (1.75, 2.0, 2.0) relative to brim top, rotX=-0.052, rotZ=0.026
            let mut cone1 = ModelPart::new([-3.25, 2.0, -3.0]).with_box(ModelBox::new(
                [0.0, 0.0, 0.0],
                [7.0, 4.0, 7.0],
                [0.0, 76.0],
                tex,
                0.0,
            ));
            cone1.rotation[0] = -0.05235988;
            cone1.rotation[2] = 0.02617994;

            // 3. Upper cone child at (1.75, 4.0, 2.0) relative to cone1, rotX=-0.105, rotZ=0.052
            let mut cone2 = ModelPart::new([1.75, 4.0, 2.0]).with_box(ModelBox::new(
                [0.0, 0.0, 0.0],
                [4.0, 4.0, 4.0],
                [0.0, 87.0],
                tex,
                0.0,
            ));
            cone2.rotation[0] = -0.10471976;
            cone2.rotation[2] = 0.05235988;

            // 4. Tip child at (1.75, 4.0, 2.0) relative to cone2, rotX=-0.209, rotZ=0.105
            let mut tip = ModelPart::new([1.75, 4.0, 2.0]).with_box(ModelBox::new(
                [0.0, 0.0, 0.0],
                [1.0, 2.0, 1.0],
                [0.0, 95.0],
                tex,
                0.25,
            ));
            tip.rotation[0] = -0.20943952;
            tip.rotation[2] = 0.10471976;

            Some(vec![brim, cone1, cone2, tip])
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
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

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
        if let Some(hat_parts) = self.witch_hat.as_ref() {
            let t_head = self.head.transform(root);
            let t_brim = hat_parts[0].transform(t_head);
            for b in &hat_parts[0].boxes {
                b.emit_triangles(t_brim, &mut out);
            }
            let t_cone1 = hat_parts[1].transform(t_brim);
            for b in &hat_parts[1].boxes {
                b.emit_triangles(t_cone1, &mut out);
            }
            let t_cone2 = hat_parts[2].transform(t_cone1);
            for b in &hat_parts[2].boxes {
                b.emit_triangles(t_cone2, &mut out);
            }
            let t_cone3 = hat_parts[3].transform(t_cone2);
            for b in &hat_parts[3].boxes {
                b.emit_triangles(t_cone3, &mut out);
            }
        }
        out
    }
}
