// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Humanoid biped model (Player, Zombie, Skeleton).

use glam::{Mat4, Vec3};

use super::child::{emit_child, ChildLayout};
use super::{ModelBox, ModelPart, ModelVertex};

/// 1.8 Humanoid model (Player, Zombie, Skeleton) supporting Steve and Alex arm widths.
#[derive(Debug, Clone)]
pub struct HumanoidModel {
    /// Head part.
    pub head: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Right arm part.
    pub right_arm: ModelPart,
    /// Left arm part.
    pub left_arm: ModelPart,
    /// Right leg part.
    pub right_leg: ModelPart,
    /// Left leg part.
    pub left_leg: ModelPart,
}

impl HumanoidModel {
    /// Builds a 1.8 player humanoid model (Steve or Alex) with base and outer layers.
    #[must_use]
    pub fn new(is_slim: bool) -> Self {
        Self::create(is_slim, false)
    }

    /// Builds a 1.8 zombie biped model with mirrored 64×32 limb textures.
    #[must_use]
    pub fn zombie() -> Self {
        Self::create(false, true)
    }

    /// Builds a 1.8 zombie villager biped model (MCP `ModelZombieVillager`).
    /// Combines a 64×64 zombie biped body/limbs with an elongated villager head and nose.
    #[must_use]
    pub fn zombie_villager() -> Self {
        let tex = [64.0, 64.0];

        // Head and nose (MCP ModelZombieVillager: texture offset 0,32 and 24,32)
        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 10.0, 8.0],
                [0.0, 32.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, -1.0, -6.0],
                [2.0, 4.0, 2.0],
                [24.0, 32.0],
                tex,
                0.0,
            ));

        let body = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -12.0, -2.0],
            [8.0, 12.0, 4.0],
            [16.0, 16.0],
            tex,
            0.0,
        ));

        let right_arm = ModelPart::new([-5.0, 22.0, 0.0]).with_box(ModelBox::new(
            [-3.0, -10.0, -2.0],
            [4.0, 12.0, 4.0],
            [40.0, 16.0],
            tex,
            0.0,
        ));

        let left_arm = ModelPart::new([5.0, 22.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -2.0],
            [4.0, 12.0, 4.0],
            [40.0, 16.0],
            tex,
            0.0,
        ));

        let right_leg = ModelPart::new([-1.9, 12.0, 0.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([1.9, 12.0, 0.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
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

    /// Builds a 1.8 skeleton biped with thin 2-pixel-wide limbs (MCP `ModelSkeleton`).
    #[must_use]
    pub fn skeleton() -> Self {
        let tex = [64.0, 32.0];

        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [32.0, 0.0],
                tex,
                0.5,
            ));

        let body = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -12.0, -2.0],
            [8.0, 12.0, 4.0],
            [16.0, 16.0],
            tex,
            0.0,
        ));

        // Thin 2px arms
        let right_arm = ModelPart::new([-5.0, 22.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -1.0],
            [2.0, 12.0, 2.0],
            [40.0, 16.0],
            tex,
            0.0,
        ));

        let left_arm = ModelPart::new([5.0, 22.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -1.0],
            [2.0, 12.0, 2.0],
            [40.0, 16.0],
            tex,
            0.0,
        ));

        // Thin 2px legs
        let right_leg = ModelPart::new([-2.0, 12.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -12.0, -1.0],
            [2.0, 12.0, 2.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        let left_leg = ModelPart::new([2.0, 12.0, 0.0]).with_box(ModelBox::new(
            [-1.0, -12.0, -1.0],
            [2.0, 12.0, 2.0],
            [0.0, 16.0],
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

    fn create(is_slim: bool, is_mob: bool) -> Self {
        let tex = [64.0, 64.0];

        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [32.0, 0.0],
                tex,
                0.5,
            ));

        let body = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, -12.0, -2.0],
                [8.0, 12.0, 4.0],
                [16.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -12.0, -2.0],
                [8.0, 12.0, 4.0],
                [16.0, 32.0],
                tex,
                0.25,
            ));

        let arm_w = if is_slim { 3.0 } else { 4.0 };
        let right_arm = ModelPart::new([-5.0, 22.0, 0.0])
            .with_box(ModelBox::new(
                [-arm_w + 1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                [40.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-arm_w + 1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                [40.0, 32.0],
                tex,
                0.25,
            ));

        let left_arm_uv = if is_mob { [40.0, 16.0] } else { [32.0, 48.0] };
        let left_arm_wear_uv = if is_mob { [40.0, 16.0] } else { [48.0, 48.0] };
        let left_arm = ModelPart::new([5.0, 22.0, 0.0])
            .with_box(ModelBox::new(
                [-1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                left_arm_uv,
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                left_arm_wear_uv,
                tex,
                0.25,
            ));

        let right_leg = ModelPart::new([-1.9, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                [0.0, 32.0],
                tex,
                0.25,
            ));

        let left_leg_uv = if is_mob { [0.0, 16.0] } else { [16.0, 48.0] };
        let left_leg_wear_uv = if is_mob { [0.0, 16.0] } else { [0.0, 48.0] };
        let left_leg = ModelPart::new([1.9, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                left_leg_uv,
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                left_leg_wear_uv,
                tex,
                0.25,
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

    /// The right arm and its sleeve as the first-person view draws them
    /// (MCP `RenderPlayer.renderRightArm`), in the vanilla model space with
    /// the y axis down and the unit a block.
    ///
    /// The model of this crate has the y axis up, so the vertices are
    /// mirrored back, and the arm gets the small roll the idle animation
    /// gives it (`ModelBiped.setRotationAngles` with an age of 0:
    /// `rotateAngleZ = 0.1`).
    #[must_use]
    pub fn first_person_arm(is_slim: bool) -> Vec<ModelVertex> {
        const PIVOT: [f32; 3] = [-5.0, 2.0, 0.0];
        const IDLE_ROLL: f32 = 0.1;
        const MODEL_HEIGHT: f32 = 24.0;

        let model = Self::new(is_slim);
        let mut out = Vec::new();
        model
            .right_arm
            .emit(Mat4::from_scale(Vec3::splat(1.0 / 16.0)), &mut out);

        let pivot = Vec3::from(PIVOT) / 16.0;
        let roll = Mat4::from_translation(pivot)
            * Mat4::from_rotation_z(IDLE_ROLL)
            * Mat4::from_translation(-pivot);
        for vertex in &mut out {
            let [x, y, z] = vertex.position;
            vertex.position = roll
                .transform_point3(Vec3::new(x, MODEL_HEIGHT / 16.0 - y, z))
                .to_array();
            let [nx, ny, nz] = vertex.normal;
            vertex.normal = roll.transform_vector3(Vec3::new(nx, -ny, nz)).to_array();
        }
        out
    }

    /// Hides the outer skin layers that `skin_parts` does not enable
    /// (MCP `EnumPlayerModelParts`: bit 1 jacket, 2 left sleeve, 3 right
    /// sleeve, 4 left pants, 5 right pants, 6 hat).
    pub fn show_skin_layers(&mut self, skin_parts: u8) {
        let layers = [
            (&mut self.body, 0x02),
            (&mut self.left_arm, 0x04),
            (&mut self.right_arm, 0x08),
            (&mut self.left_leg, 0x10),
            (&mut self.right_leg, 0x20),
            (&mut self.head, 0x40),
        ];
        for (part, bit) in layers {
            if skin_parts & bit == 0 {
                // The first box is the skin, the second the layer over it.
                part.boxes.truncate(1);
            }
        }
    }

    /// Keeps only the head and its hat, as `RenderPlayer.setModelVisibilities`
    /// does for a player in spectator mode.
    pub fn keep_head_only(&mut self) {
        for part in [
            &mut self.body,
            &mut self.right_arm,
            &mut self.left_arm,
            &mut self.right_leg,
            &mut self.left_leg,
        ] {
            part.boxes.clear();
        }
    }

    /// Emits a baby: the head at three quarters, the rest halved
    /// (MCP `ModelBiped.render` with `isChild`).
    #[must_use]
    pub fn emit_child_mesh(&self) -> Vec<ModelVertex> {
        emit_child(
            &[&self.head],
            &[
                &self.body,
                &self.right_arm,
                &self.left_arm,
                &self.right_leg,
                &self.left_leg,
            ],
            ChildLayout::BIPED,
        )
    }

    /// Generates the complete vertex mesh in blocks (1 unit = 1/16 block).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(vertices: &[ModelVertex], axis: usize) -> (f32, f32) {
        let values = vertices.iter().map(|vertex| vertex.position[axis]);
        (
            values.clone().fold(f32::MAX, f32::min),
            values.fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn the_skin_layers_follow_the_skin_parts() {
        let all = HumanoidModel::new(false);
        let mut none = HumanoidModel::new(false);
        none.show_skin_layers(0);
        assert_eq!(none.emit_mesh().len(), all.emit_mesh().len() / 2);
        let mut hat_only = HumanoidModel::new(false);
        hat_only.show_skin_layers(0x40);
        assert_eq!(hat_only.head.boxes.len(), 2);
        assert_eq!(hat_only.body.boxes.len(), 1);
        let mut everything = HumanoidModel::new(false);
        everything.show_skin_layers(0x7F);
        assert_eq!(everything.emit_mesh().len(), all.emit_mesh().len());
    }

    #[test]
    fn the_first_person_arm_is_the_arm_and_its_sleeve() {
        let arm = HumanoidModel::first_person_arm(false);
        assert_eq!(arm.len(), 72, "two boxes of 36 vertices");
    }

    #[test]
    fn the_first_person_arm_hangs_down_from_the_shoulder_in_vanilla_space() {
        let arm = HumanoidModel::first_person_arm(false);
        // Vanilla space: y down, the arm spans 12 pixels below the shoulder
        // line (0.75 block) plus the sleeve dilation of 0.25 pixel.
        let (top, bottom) = extent(&arm, 1);
        assert!(top < 0.0 && top > -0.05, "{top}");
        assert!((bottom - 0.75).abs() < 0.06, "{bottom}");
        // The arm is 4 pixels wide, to the right of the body (x < 0).
        let (left, right) = extent(&arm, 0);
        assert!(left > -0.6 && right < -0.2, "{left} {right}");
        assert!(right - left > 0.24);
    }

    #[test]
    fn the_slim_arm_is_one_pixel_narrower() {
        let wide = extent(&HumanoidModel::first_person_arm(false), 0);
        let slim = extent(&HumanoidModel::first_person_arm(true), 0);
        assert!(((wide.1 - wide.0) - (slim.1 - slim.0) - 1.0 / 16.0).abs() < 0.01);
    }

    #[test]
    fn the_arm_normals_stay_unit_length() {
        for vertex in HumanoidModel::first_person_arm(false) {
            let length = Vec3::from(vertex.normal).length();
            assert!((length - 1.0).abs() < 1e-4, "{length}");
        }
    }
}
