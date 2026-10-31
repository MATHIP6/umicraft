// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wolf / Dog box model matching MCP 1.8 `ModelWolf.java`.
//!
//! The parts keep the pivots and boxes of the reference: a vanilla pivot
//! `(x, y, z)` is `(x, 24 - y, z)` here, a vanilla box `(x, y, z, w, h, d)` has
//! its origin at `(x, -(y + h), z)`, and a vanilla rotation `(x, y, z)` is
//! `(-x, y, -z)` because the y axis is flipped.

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::child::{emit_child, ChildLayout};
use super::{ModelBox, ModelPart, ModelVertex};

/// Tail angle of a wild wolf (MCP `EntityWolf.getTailRotation`).
const WILD_TAIL: f32 = PI / 5.0;
/// Tail angle of an angry wolf.
const ANGRY_TAIL: f32 = 1.539_380_4;
/// Head tilt of a begging wolf (`getInterestedAngle` at full interest).
const BEGGING_TILT: f32 = 0.15 * PI;

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
    sitting: bool,
    angry: bool,
    begging: bool,
    tail_angle: f32,
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

impl WolfModel {
    /// Creates a new Wolf model.
    #[must_use]
    pub fn new() -> Self {
        let leg = |x: f32, z: f32| {
            part([x, 16.0, z]).with_box(vanilla_box(
                [-1.0, 0.0, -1.0],
                [2.0, 8.0, 2.0],
                [0.0, 18.0],
            ))
        };
        let head = part([-1.0, 13.5, -7.0])
            .with_box(vanilla_box([-3.0, -3.0, -2.0], [6.0, 6.0, 4.0], [0.0, 0.0]))
            .with_box(vanilla_box(
                [-3.0, -5.0, 0.0],
                [2.0, 2.0, 1.0],
                [16.0, 14.0],
            ))
            .with_box(vanilla_box([1.0, -5.0, 0.0], [2.0, 2.0, 1.0], [16.0, 14.0]))
            .with_box(vanilla_box([-1.5, 0.0, -5.0], [3.0, 3.0, 4.0], [0.0, 10.0]));
        let body = part([0.0, 14.0, 2.0]).with_box(vanilla_box(
            [-4.0, -2.0, -3.0],
            [6.0, 9.0, 6.0],
            [18.0, 14.0],
        ));
        let mane = part([-1.0, 14.0, -3.0]).with_box(vanilla_box(
            [-4.0, -3.0, -3.0],
            [8.0, 6.0, 7.0],
            [21.0, 0.0],
        ));
        let tail = part([-1.0, 12.0, 8.0]).with_box(vanilla_box(
            [-1.0, 0.0, -1.0],
            [2.0, 8.0, 2.0],
            [9.0, 18.0],
        ));
        Self {
            head,
            body,
            mane,
            leg1: leg(-2.5, 7.0),
            leg2: leg(0.5, 7.0),
            leg3: leg(-2.5, -4.0),
            leg4: leg(0.5, -4.0),
            tail,
            sitting: false,
            angry: false,
            begging: false,
            tail_angle: WILD_TAIL,
        }
    }

    /// Sets what the metadata says: sitting, angry, begging, and for a tamed
    /// wolf its health (the tail hangs lower as it drops; `None` for a wild
    /// wolf). Call before [`Self::set_angles`].
    pub fn set_state(
        &mut self,
        sitting: bool,
        angry: bool,
        begging: bool,
        tamed_health: Option<f32>,
    ) {
        self.sitting = sitting;
        self.angry = angry;
        self.begging = begging;
        // MCP `EntityWolf.getTailRotation`.
        self.tail_angle = if angry {
            ANGRY_TAIL
        } else if let Some(health) = tamed_health {
            (0.55 - (20.0 - health) * 0.02) * PI
        } else {
            WILD_TAIL
        };
    }

    /// Sets walking animation angles (MCP `ModelWolf.setLivingAnimations` and
    /// `setRotationAngles`).
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        let swing = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        let swing_back = (limb_swing * 0.6662 + PI).cos() * 1.4 * limb_swing_amount;
        self.tail.rotation[1] = if self.angry { 0.0 } else { swing };
        if self.sitting {
            self.mane.pivot = [-1.0, 8.0, -3.0];
            self.mane.rotation[0] = -(PI * 2.0 / 5.0);
            self.body.pivot = [0.0, 6.0, 0.0];
            self.body.rotation[0] = -(PI / 4.0);
            self.tail.pivot = [-1.0, 3.0, 6.0];
            self.leg1.pivot = [-2.5, 2.0, 2.0];
            self.leg1.rotation[0] = -(PI * 3.0 / 2.0);
            self.leg2.pivot = [0.5, 2.0, 2.0];
            self.leg2.rotation[0] = -(PI * 3.0 / 2.0);
            self.leg3.pivot = [-2.49, 7.0, -4.0];
            self.leg3.rotation[0] = -5.811_947;
            self.leg4.pivot = [0.51, 7.0, -4.0];
            self.leg4.rotation[0] = -5.811_947;
        } else {
            self.body.pivot = [0.0, 10.0, 2.0];
            self.body.rotation[0] = -(PI / 2.0);
            self.mane.pivot = [-1.0, 10.0, -3.0];
            self.mane.rotation[0] = self.body.rotation[0];
            self.tail.pivot = [-1.0, 12.0, 8.0];
            self.leg1.pivot = [-2.5, 8.0, 7.0];
            self.leg2.pivot = [0.5, 8.0, 7.0];
            self.leg3.pivot = [-2.5, 8.0, -4.0];
            self.leg4.pivot = [0.5, 8.0, -4.0];
            self.leg1.rotation[0] = -swing;
            self.leg2.rotation[0] = -swing_back;
            self.leg3.rotation[0] = -swing_back;
            self.leg4.rotation[0] = -swing;
        }
        // The vanilla roll of the head is a begging tilt (the shake is not
        // tracked), negated with the y axis.
        self.head.rotation[2] = if self.begging { -BEGGING_TILT } else { 0.0 };
        self.head.rotation[0] = -head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();
        self.tail.rotation[0] = -self.tail_angle;
    }

    /// Emits a pup: the head at full size, the rest halved.
    #[must_use]
    pub fn emit_child_mesh(&self) -> Vec<ModelVertex> {
        emit_child(
            &[&self.head],
            &[
                &self.body, &self.mane, &self.leg1, &self.leg2, &self.leg3, &self.leg4, &self.tail,
            ],
            ChildLayout::SMALL_ANIMAL,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn top(model: &WolfModel) -> f32 {
        model
            .emit_mesh()
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn a_sitting_wolf_is_lower_and_its_legs_fold() {
        let mut standing = WolfModel::new();
        standing.set_angles(0.0, 0.0, 0.0, 0.0);
        let mut sitting = WolfModel::new();
        sitting.set_state(true, false, false, None);
        sitting.set_angles(0.0, 0.0, 0.0, 0.0);
        assert!(sitting.body.pivot[1] < standing.body.pivot[1]);
        assert_ne!(sitting.leg1.rotation[0], standing.leg1.rotation[0]);
        assert_eq!(sitting.emit_mesh().len(), standing.emit_mesh().len());
        assert!(top(&sitting) <= top(&standing) + 1e-4);
    }

    #[test]
    fn a_walking_wolf_swings_diagonal_legs_together_and_a_sitting_one_does_not() {
        let mut walking = WolfModel::new();
        walking.set_angles(1.0, 1.0, 0.0, 0.0);
        assert_eq!(walking.leg1.rotation[0], walking.leg4.rotation[0]);
        assert_eq!(walking.leg2.rotation[0], walking.leg3.rotation[0]);
        assert_ne!(walking.leg1.rotation[0], walking.leg2.rotation[0]);
        let mut moving = WolfModel::new();
        moving.set_state(true, false, false, None);
        moving.set_angles(1.0, 1.0, 0.0, 0.0);
        let mut still = WolfModel::new();
        still.set_state(true, false, false, None);
        still.set_angles(0.0, 0.0, 0.0, 0.0);
        assert_eq!(moving.leg1.rotation, still.leg1.rotation);
    }

    #[test]
    fn the_tail_follows_the_mood_and_the_health() {
        let tail = |angry, health: Option<f32>| {
            let mut wolf = WolfModel::new();
            wolf.set_state(false, angry, false, health);
            wolf.set_angles(0.0, 0.0, 0.0, 0.0);
            -wolf.tail.rotation[0]
        };
        assert!((tail(false, None) - PI / 5.0).abs() < 1e-5);
        assert!((tail(true, Some(20.0)) - ANGRY_TAIL).abs() < 1e-5);
        assert!((tail(false, Some(20.0)) - 0.55 * PI).abs() < 1e-5);
        assert!(
            tail(false, Some(4.0)) < tail(false, Some(20.0)),
            "a hurt wolf droops"
        );
    }

    #[test]
    fn a_begging_wolf_tilts_its_head() {
        let mut wolf = WolfModel::new();
        wolf.set_state(false, false, true, None);
        wolf.set_angles(0.0, 0.0, 0.0, 0.0);
        assert_ne!(wolf.head.rotation[2], 0.0);
    }
}
