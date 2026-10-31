// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ocelot / Cat box model matching MCP 1.8 `ModelOcelot.java`.
//!
//! Pivots, boxes and angles are those of the reference converted for the y
//! axis flip: a vanilla pivot `(x, y, z)` is `(x, 24 - y, z)`, a vanilla box
//! `(x, y, z, w, h, d)` starts at `(x, -(y + h), z)` and a vanilla rotation
//! about x is negated.

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::child::{emit_child, ChildLayout};
use super::{ModelBox, ModelPart, ModelVertex};

/// What the ocelot does (MCP `ModelOcelot.field_78163_i`, `setLivingAnimations`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OcelotPose {
    /// Walking about.
    #[default]
    Standing,
    /// Sneaking (the crouch flag).
    Sneaking,
    /// Sprinting (the sprint flag).
    Sprinting,
    /// Sitting down (tamed and told to sit).
    Sitting,
}

/// Ocelot / Cat model (64×32 texture sheet) with head, body, 4 legs and jointed tail.
#[derive(Debug, Clone)]
pub struct OcelotModel {
    /// Head part with ears and muzzle.
    pub head: ModelPart,
    /// Torso / body.
    pub body: ModelPart,
    /// Back left leg.
    pub back_left_leg: ModelPart,
    /// Back right leg.
    pub back_right_leg: ModelPart,
    /// Front left leg.
    pub front_left_leg: ModelPart,
    /// Front right leg.
    pub front_right_leg: ModelPart,
    /// Upper tail segment.
    pub tail1: ModelPart,
    /// Lower tail segment.
    pub tail2: ModelPart,
    pose: OcelotPose,
}

/// A vanilla box as a box of this crate (the y axis flipped).
fn vanilla_box(origin: [f32; 3], size: [f32; 3], uv: [f32; 2]) -> ModelBox {
    ModelBox::new(
        [origin[0], -(origin[1] + size[1]), origin[2]],
        size,
        uv,
        [64.0, 32.0],
        0.0,
    )
}

/// A part at the vanilla pivot `(x, y, z)`.
fn part(pivot: [f32; 3]) -> ModelPart {
    ModelPart::new([pivot[0], 24.0 - pivot[1], pivot[2]])
}

/// Moves a part to the vanilla pivot `(x, y, z)`.
fn place(part: &mut ModelPart, pivot: [f32; 3]) {
    part.pivot = [pivot[0], 24.0 - pivot[1], pivot[2]];
}

impl OcelotModel {
    /// Creates a new ocelot model.
    #[must_use]
    pub fn new() -> Self {
        let head = part([0.0, 15.0, -9.0])
            .with_box(vanilla_box([-2.5, -2.0, -3.0], [5.0, 4.0, 5.0], [0.0, 0.0]))
            .with_box(vanilla_box([-1.5, 0.0, -4.0], [3.0, 2.0, 2.0], [0.0, 24.0]))
            .with_box(vanilla_box([-2.0, -3.0, 0.0], [1.0, 1.0, 2.0], [0.0, 10.0]))
            .with_box(vanilla_box([1.0, -3.0, 0.0], [1.0, 1.0, 2.0], [6.0, 10.0]));
        let body = part([0.0, 12.0, -10.0]).with_box(vanilla_box(
            [-2.0, 3.0, -8.0],
            [4.0, 16.0, 6.0],
            [20.0, 0.0],
        ));
        let leg = |x: f32, y: f32, z: f32, box_z: f32, height: f32, uv: [f32; 2]| {
            part([x, y, z]).with_box(vanilla_box([-1.0, 0.0, box_z], [2.0, height, 2.0], uv))
        };
        let tail = |pivot: [f32; 3], uv: [f32; 2]| {
            part(pivot).with_box(vanilla_box([-0.5, 0.0, 0.0], [1.0, 8.0, 1.0], uv))
        };
        let mut model = Self {
            head,
            body,
            back_left_leg: leg(1.1, 18.0, 5.0, 1.0, 6.0, [8.0, 13.0]),
            back_right_leg: leg(-1.1, 18.0, 5.0, 1.0, 6.0, [8.0, 13.0]),
            front_left_leg: leg(1.2, 13.8, -5.0, 0.0, 10.0, [40.0, 0.0]),
            front_right_leg: leg(-1.2, 13.8, -5.0, 0.0, 10.0, [40.0, 0.0]),
            tail1: tail([0.0, 15.0, 8.0], [0.0, 15.0]),
            tail2: tail([0.0, 20.0, 14.0], [4.0, 15.0]),
            pose: OcelotPose::Standing,
        };
        model.set_angles(0.0, 0.0, 0.0, 0.0);
        model
    }

    /// Sets the pose from the entity flags and the sitting metadata. Call
    /// before [`Self::set_angles`].
    pub fn set_pose(&mut self, pose: OcelotPose) {
        self.pose = pose;
    }

    /// The pivots and the tail angles of the pose (MCP `setLivingAnimations`).
    fn apply_pose(&mut self) {
        place(&mut self.body, [0.0, 12.0, -10.0]);
        place(&mut self.head, [0.0, 15.0, -9.0]);
        place(&mut self.tail1, [0.0, 15.0, 8.0]);
        place(&mut self.tail2, [0.0, 20.0, 14.0]);
        for leg in [&mut self.front_left_leg, &mut self.front_right_leg] {
            place(leg, [leg.pivot[0], 13.8, -5.0]);
        }
        for leg in [&mut self.back_left_leg, &mut self.back_right_leg] {
            place(leg, [leg.pivot[0], 18.0, 5.0]);
        }
        self.tail1.rotation[0] = -0.9;
        match self.pose {
            OcelotPose::Sneaking => {
                place(&mut self.body, [0.0, 13.0, -10.0]);
                place(&mut self.head, [0.0, 17.0, -9.0]);
                place(&mut self.tail1, [0.0, 16.0, 8.0]);
                place(&mut self.tail2, [0.0, 16.0, 16.0]);
                self.tail1.rotation[0] = -PI / 2.0;
                self.tail2.rotation[0] = -PI / 2.0;
            }
            OcelotPose::Sprinting => {
                place(&mut self.tail2, [0.0, 15.0, 16.0]);
                self.tail1.rotation[0] = -PI / 2.0;
                self.tail2.rotation[0] = -PI / 2.0;
            }
            OcelotPose::Sitting => {
                self.body.rotation[0] = -PI / 4.0;
                place(&mut self.body, [0.0, 8.0, -5.0]);
                place(&mut self.head, [0.0, 11.7, -8.0]);
                place(&mut self.tail1, [0.0, 23.0, 6.0]);
                place(&mut self.tail2, [0.0, 22.0, 13.2]);
                self.tail1.rotation[0] = -1.727_876_1;
                self.tail2.rotation[0] = -2.670_354;
                for leg in [&mut self.front_left_leg, &mut self.front_right_leg] {
                    leg.rotation[0] = 0.157_079_64;
                    place(leg, [leg.pivot[0], 15.8, -7.0]);
                }
                for leg in [&mut self.back_left_leg, &mut self.back_right_leg] {
                    leg.rotation[0] = PI / 2.0;
                    place(leg, [leg.pivot[0], 21.0, 1.0]);
                }
            }
            OcelotPose::Standing => {}
        }
    }

    /// Sets walk cycle animation angles (MCP `ModelOcelot.setRotationAngles`).
    pub fn set_angles(
        &mut self,
        limb_angle: f32,
        limb_distance: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.apply_pose();
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();
        if self.pose == OcelotPose::Sitting {
            return;
        }
        self.body.rotation[0] = -PI / 2.0;
        let phase = limb_angle * 0.6662;
        let amount = limb_distance;
        let (back_left, back_right, front_left, front_right) = if self.pose == OcelotPose::Sprinting
        {
            (
                phase.cos() * amount,
                (phase + 0.3).cos() * amount,
                (phase + PI + 0.3).cos() * amount,
                (phase + PI).cos() * amount,
            )
        } else {
            (
                phase.cos() * amount,
                (phase + PI).cos() * amount,
                (phase + PI).cos() * amount,
                phase.cos() * amount,
            )
        };
        self.back_left_leg.rotation[0] = -back_left;
        self.back_right_leg.rotation[0] = -back_right;
        self.front_left_leg.rotation[0] = -front_left;
        self.front_right_leg.rotation[0] = -front_right;
        let tail_swing = match self.pose {
            OcelotPose::Sprinting => PI / 10.0,
            OcelotPose::Standing => PI / 4.0,
            _ => 0.471_238_94,
        };
        self.tail2.rotation[0] = -(1.727_876_1 + tail_swing * limb_angle.cos() * amount);
    }

    /// Emits a kitten: the head at three quarters, the rest halved.
    #[must_use]
    pub fn emit_child_mesh(&self) -> Vec<ModelVertex> {
        emit_child(
            &[&self.head],
            &[
                &self.body,
                &self.back_left_leg,
                &self.back_right_leg,
                &self.front_left_leg,
                &self.front_right_leg,
                &self.tail1,
                &self.tail2,
            ],
            ChildLayout::OCELOT,
        )
    }

    /// Emits the ocelot mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.back_left_leg.emit(root, &mut out);
        self.back_right_leg.emit(root, &mut out);
        self.front_left_leg.emit(root, &mut out);
        self.front_right_leg.emit(root, &mut out);
        self.tail1.emit(root, &mut out);
        self.tail2.emit(root, &mut out);

        out
    }
}

impl Default for OcelotModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posed(pose: OcelotPose, swing: f32, amount: f32) -> OcelotModel {
        let mut model = OcelotModel::new();
        model.set_pose(pose);
        model.set_angles(swing, amount, 0.0, 0.0);
        model
    }

    #[test]
    fn every_pose_draws_the_same_boxes() {
        let count = posed(OcelotPose::Standing, 0.0, 0.0).emit_mesh().len();
        for pose in [
            OcelotPose::Sneaking,
            OcelotPose::Sprinting,
            OcelotPose::Sitting,
        ] {
            assert_eq!(posed(pose, 0.0, 0.0).emit_mesh().len(), count);
        }
        // Head 4 boxes, body, 4 legs, 2 tail segments.
        assert_eq!(count, 11 * 36);
    }

    #[test]
    fn a_sitting_cat_tilts_its_body_and_folds_its_back_legs() {
        let standing = posed(OcelotPose::Standing, 0.0, 0.0);
        let sitting = posed(OcelotPose::Sitting, 0.0, 0.0);
        assert_ne!(sitting.body.rotation[0], standing.body.rotation[0]);
        assert_ne!(
            sitting.back_left_leg.rotation[0],
            standing.back_left_leg.rotation[0]
        );
        // A sitting cat does not walk in place.
        let moving = posed(OcelotPose::Sitting, 2.0, 1.0);
        assert_eq!(
            moving.back_left_leg.rotation,
            sitting.back_left_leg.rotation
        );
    }

    #[test]
    fn a_sneaking_cat_is_lower_and_a_sprinting_one_stretches_its_tail() {
        let standing = posed(OcelotPose::Standing, 0.0, 0.0);
        let sneaking = posed(OcelotPose::Sneaking, 0.0, 0.0);
        assert!(sneaking.head.pivot[1] < standing.head.pivot[1]);
        let sprinting = posed(OcelotPose::Sprinting, 0.0, 0.0);
        assert_eq!(sprinting.tail1.rotation[0], -PI / 2.0);
    }

    #[test]
    fn a_walking_cat_swings_the_diagonal_legs_together() {
        let walking = posed(OcelotPose::Standing, 1.0, 1.0);
        assert_eq!(
            walking.back_left_leg.rotation[0],
            walking.front_right_leg.rotation[0]
        );
        assert_ne!(
            walking.back_left_leg.rotation[0],
            walking.back_right_leg.rotation[0]
        );
    }
}
