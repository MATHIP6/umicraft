// SPDX-License-Identifier: MIT OR Apache-2.0

//! The armour stand: a port of MCP `ModelArmorStand` and the posing of
//! `ModelArmorStandArmor`.
//!
//! The stand is a thin biped (head stick, shoulder bar, arm and leg sticks)
//! plus four parts of its own: the two side sticks and the waist, which turn
//! with the body, and the stone base plate. Every angle comes from the
//! metadata of the entity, none from its movement. The armour it wears is the
//! ordinary biped shell posed with the same angles ([`ArmorStandPose::pose_biped`]).
//!
//! Models of this crate are the vanilla ones with y flipped (y up, pivots at
//! `24 - y`), so a vanilla box `addBox(x, y, z, w, h, d)` has its origin at
//! `(x, -(y + h), z)` here.

use glam::{Mat4, Vec3};

use super::child::{emit_child, ChildLayout};
use super::pose::apply;
use super::{HumanoidModel, ModelBox, ModelPart, ModelVertex};

/// The texture of the stand is 64×64 (`ModelArmorStand(0)` calls
/// `super(size, 64, 64)`).
const TEXTURE: [f32; 2] = [64.0, 64.0];
/// Height of the head pivot: `setRotationPoint(0, 1, 0)` in
/// `ModelArmorStandArmor.setRotationAngles`.
const HEAD_PIVOT_Y: f32 = 24.0 - 1.0;
/// Height of the leg pivots of the stand (`ModelArmorStand` puts them back at
/// 12 after the armour posing set 11).
const STAND_LEG_PIVOT_Y: f32 = 24.0 - 12.0;
/// Height of the leg pivots of the armour worn by a stand
/// (`ModelArmorStandArmor`: `setRotationPoint(±1.9, 11, 0)`).
pub const ARMOR_LEG_PIVOT_Y: f32 = 24.0 - 11.0;

/// The angles of an armour stand, in degrees about x, y and z as the
/// metadata sends them (entries 11 to 16), with its yaw and its flags.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArmorStandPose {
    /// Head rotation.
    pub head: [f32; 3],
    /// Body rotation, shared by the side sticks and the waist.
    pub body: [f32; 3],
    /// Left arm rotation.
    pub left_arm: [f32; 3],
    /// Right arm rotation.
    pub right_arm: [f32; 3],
    /// Left leg rotation.
    pub left_leg: [f32; 3],
    /// Right leg rotation.
    pub right_leg: [f32; 3],
    /// The yaw of the entity in degrees, which the base plate undoes so that
    /// it stays square with the world (`standBase.rotateAngleY = -yaw`).
    pub yaw: f32,
    /// The arms are drawn (flag 4).
    pub arms: bool,
    /// The base plate is drawn (flag 8 unset).
    pub base_plate: bool,
}

impl ArmorStandPose {
    /// Poses a biped from these angles (MCP
    /// `ModelArmorStandArmor.setRotationAngles`): every part takes its angle
    /// from the metadata, the head pivot moves to 1 and the legs to
    /// `leg_pivot_y` ([`ARMOR_LEG_PIVOT_Y`] for armour).
    pub fn pose_biped(&self, model: &mut HumanoidModel, leg_pivot_y: f32) {
        apply(&mut model.head, radians(self.head));
        model.head.pivot = [0.0, HEAD_PIVOT_Y, 0.0];
        apply(&mut model.body, radians(self.body));
        apply(&mut model.left_arm, radians(self.left_arm));
        apply(&mut model.right_arm, radians(self.right_arm));
        apply(&mut model.left_leg, radians(self.left_leg));
        apply(&mut model.right_leg, radians(self.right_leg));
        model.left_leg.pivot = [1.9, leg_pivot_y, 0.0];
        model.right_leg.pivot = [-1.9, leg_pivot_y, 0.0];
    }
}

/// Degrees to radians, component-wise (`0.017453292F * angle`).
fn radians(degrees: [f32; 3]) -> [f32; 3] {
    degrees.map(f32::to_radians)
}

/// The armour stand model (MCP `ModelArmorStand`).
#[derive(Debug, Clone)]
pub struct ArmorStandModel {
    /// The biped parts: head (with the unused headwear of `ModelBiped`),
    /// shoulder bar, arms and legs.
    pub biped: HumanoidModel,
    /// Right side stick under the shoulders.
    pub right_side: ModelPart,
    /// Left side stick under the shoulders.
    pub left_side: ModelPart,
    /// Waist bar.
    pub waist: ModelPart,
    /// Stone base plate.
    pub base: ModelPart,
    /// Whether the arms are drawn.
    pub arms: bool,
    /// Whether the base plate is drawn.
    pub base_plate: bool,
}

impl Default for ArmorStandModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ArmorStandModel {
    /// Builds the unposed model, arms hidden and base plate shown, as a new
    /// stand is.
    #[must_use]
    pub fn new() -> Self {
        let part = |pivot: [f32; 3], origin: [f32; 3], size: [f32; 3], uv: [f32; 2]| {
            ModelPart::new(pivot).with_box(ModelBox::new(origin, size, uv, TEXTURE, 0.0))
        };
        // `ModelBiped(0, 0, 64, 64)` builds the headwear, which
        // `ModelArmorStand` keeps; its texels are transparent on the stand
        // texture, but a pack may paint them.
        let head = part(
            [0.0, HEAD_PIVOT_Y, 0.0],
            [-1.0, 0.0, -1.0],
            [2.0, 7.0, 2.0],
            [0.0, 0.0],
        )
        .with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0; 3],
            [32.0, 0.0],
            TEXTURE,
            0.5,
        ));
        let mirrored = |pivot, origin, size, uv| {
            ModelPart::new(pivot).with_box(ModelBox::new(origin, size, uv, TEXTURE, 0.0).mirrored())
        };
        let biped = HumanoidModel {
            head,
            body: part(
                [0.0, 24.0, 0.0],
                [-6.0, -3.0, -1.5],
                [12.0, 3.0, 3.0],
                [0.0, 26.0],
            ),
            right_arm: part(
                [-5.0, 22.0, 0.0],
                [-2.0, -10.0, -1.0],
                [2.0, 12.0, 2.0],
                [24.0, 0.0],
            ),
            left_arm: mirrored(
                [5.0, 22.0, 0.0],
                [0.0, -10.0, -1.0],
                [2.0, 12.0, 2.0],
                [32.0, 16.0],
            ),
            right_leg: part(
                [-1.9, STAND_LEG_PIVOT_Y, 0.0],
                [-1.0, -11.0, -1.0],
                [2.0, 11.0, 2.0],
                [8.0, 0.0],
            ),
            left_leg: mirrored(
                [1.9, STAND_LEG_PIVOT_Y, 0.0],
                [-1.0, -11.0, -1.0],
                [2.0, 11.0, 2.0],
                [40.0, 16.0],
            ),
        };
        Self {
            biped,
            right_side: part(
                [0.0, 24.0, 0.0],
                [-3.0, -10.0, -1.0],
                [2.0, 7.0, 2.0],
                [16.0, 0.0],
            ),
            left_side: part(
                [0.0, 24.0, 0.0],
                [1.0, -10.0, -1.0],
                [2.0, 7.0, 2.0],
                [48.0, 16.0],
            ),
            waist: part(
                [0.0, 24.0, 0.0],
                [-4.0, -12.0, -1.0],
                [8.0, 2.0, 2.0],
                [0.0, 48.0],
            ),
            base: part(
                [0.0, 12.0, 0.0],
                [-6.0, -12.0, -6.0],
                [12.0, 1.0, 12.0],
                [0.0, 32.0],
            ),
            arms: false,
            base_plate: true,
        }
    }

    /// Poses the stand (MCP `ModelArmorStand.setRotationAngles`): the biped
    /// takes the metadata angles, the side sticks and the waist turn with the
    /// body, and the base plate undoes the yaw of the entity.
    pub fn set_pose(&mut self, pose: &ArmorStandPose) {
        pose.pose_biped(&mut self.biped, STAND_LEG_PIVOT_Y);
        for part in [&mut self.right_side, &mut self.left_side, &mut self.waist] {
            apply(part, radians(pose.body));
        }
        apply(&mut self.base, [0.0, -pose.yaw.to_radians(), 0.0]);
        self.arms = pose.arms;
        self.base_plate = pose.base_plate;
    }

    /// The parts drawn below the head, in the order of `ModelBiped.render`
    /// then `ModelArmorStand.render`, skipping the hidden ones.
    fn shown_body(&self) -> Vec<&ModelPart> {
        let b = &self.biped;
        let mut parts = vec![&b.body];
        if self.arms {
            parts.extend([&b.right_arm, &b.left_arm]);
        }
        parts.extend([
            &b.right_leg,
            &b.left_leg,
            &self.right_side,
            &self.left_side,
            &self.waist,
        ]);
        if self.base_plate {
            parts.push(&self.base);
        }
        parts
    }

    /// The vertices of the stand in blocks.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.biped.head.emit(root, &mut out);
        for part in self.shown_body() {
            part.emit(root, &mut out);
        }
        out
    }

    /// The vertices of a small stand: `ModelBiped.render` draws the biped of
    /// a child and `ModelArmorStand.render` halves its own parts the same way
    /// (`scale(1/2)`, `translate(0, 24 * scale, 0)`).
    #[must_use]
    pub fn emit_child_mesh(&self) -> Vec<ModelVertex> {
        emit_child(&[&self.biped.head], &self.shown_body(), ChildLayout::BIPED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The poses of a new stand (MCP `EntityArmorStand.DEFAULT_*_ROTATION`).
    fn default_pose() -> ArmorStandPose {
        ArmorStandPose {
            left_arm: [-10.0, 0.0, -10.0],
            right_arm: [-15.0, 0.0, 10.0],
            left_leg: [-1.0, 0.0, -1.0],
            right_leg: [1.0, 0.0, 1.0],
            base_plate: true,
            ..ArmorStandPose::default()
        }
    }

    fn boxes(model: &ArmorStandModel) -> usize {
        model.emit_mesh().len() / 36
    }

    #[test]
    fn a_new_stand_has_no_arms_and_a_base_plate() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&default_pose());
        // Head stick and headwear, shoulders, two legs, two sides, waist, base.
        assert_eq!(boxes(&model), 9);
    }

    #[test]
    fn the_flags_show_the_arms_and_hide_the_plate() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&ArmorStandPose {
            arms: true,
            base_plate: false,
            ..default_pose()
        });
        assert_eq!(boxes(&model), 10, "two arms in, the plate out");
    }

    #[test]
    fn the_angles_are_the_metadata_degrees_flipped_into_y_up() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&ArmorStandPose {
            head: [10.0, 20.0, 30.0],
            ..default_pose()
        });
        let [x, y, z] = model.biped.head.rotation;
        assert!((x + 10f32.to_radians()).abs() < 1e-6);
        assert!((y - 20f32.to_radians()).abs() < 1e-6);
        assert!((z + 30f32.to_radians()).abs() < 1e-6);
        assert_eq!(model.biped.head.pivot, [0.0, 23.0, 0.0]);
    }

    #[test]
    fn the_sides_and_waist_turn_with_the_body() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&ArmorStandPose {
            body: [0.0, 45.0, 0.0],
            ..default_pose()
        });
        let body = model.biped.body.rotation;
        assert_eq!(model.right_side.rotation, body);
        assert_eq!(model.left_side.rotation, body);
        assert_eq!(model.waist.rotation, body);
    }

    #[test]
    fn the_plate_undoes_the_yaw_of_the_entity() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&ArmorStandPose {
            yaw: 90.0,
            ..default_pose()
        });
        assert!((model.base.rotation[1] + 90f32.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn the_stand_legs_pivot_at_12_and_the_armour_legs_at_11() {
        let mut stand = ArmorStandModel::new();
        stand.set_pose(&default_pose());
        assert_eq!(stand.biped.left_leg.pivot, [1.9, 12.0, 0.0]);
        let mut armor = HumanoidModel::armor(1);
        default_pose().pose_biped(&mut armor, ARMOR_LEG_PIVOT_Y);
        assert_eq!(armor.right_leg.pivot, [-1.9, 13.0, 0.0]);
    }

    #[test]
    fn the_plate_lies_on_the_ground_and_the_head_reaches_two_blocks() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&default_pose());
        let ys = model.emit_mesh();
        let low = ys.iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        assert!(
            low.abs() < 1e-5,
            "the plate's underside is at the feet: {low}"
        );
        let stick_top = 23.0 / 16.0 + 7.0 / 16.0;
        let high = ys.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        // The headwear (8 + 1 dilated, centred on the head stick) is the top.
        assert!(high > stick_top, "{high}");
    }

    #[test]
    fn a_small_stand_is_half_as_tall() {
        let mut model = ArmorStandModel::new();
        model.set_pose(&default_pose());
        let top =
            |mesh: Vec<ModelVertex>| mesh.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        let low =
            |mesh: Vec<ModelVertex>| mesh.iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        assert!(top(model.emit_child_mesh()) < top(model.emit_mesh()));
        assert!(
            low(model.emit_child_mesh()).abs() < 1e-5,
            "still on the ground"
        );
    }
}
