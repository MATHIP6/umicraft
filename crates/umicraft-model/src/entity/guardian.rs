// SPDX-License-Identifier: MIT OR Apache-2.0

//! Guardian and Elder Guardian box models matching MCP 1.8 `ModelGuardian.java`.
//!
//! The vanilla model is one body part carrying the spines, the eye and a
//! chain of three tail segments, all turned with the look. Here it is the
//! same hierarchy with the y axis flipped: a root pivot `y` becomes `24 - y`,
//! a child pivot `y` becomes `-y`, a box `[y, y + h]` becomes `[-y - h, -y]`,
//! and x and z rotations change sign.

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Height of the body pivot (vanilla `0`).
const ROOT_Y: f32 = 24.0;
/// Scale of an elder guardian (MCP `RenderGuardian.preRenderCallback`).
const ELDER_SCALE: f32 = 2.35;
/// The spines' turns, in half turns, and their offsets from the centre of
/// the body (`ModelGuardian.setRotationAngles`).
const SPINE_TURN_X: [f32; 12] = [
    1.75, 0.25, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.25, 0.75, 0.0, 0.0,
];
const SPINE_TURN_Y: [f32; 12] = [
    0.0, 0.0, 0.0, 0.0, 0.25, 1.75, 1.25, 0.75, 0.0, 0.0, 0.0, 0.0,
];
const SPINE_TURN_Z: [f32; 12] = [
    0.0, 0.0, 0.25, 1.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, 1.25,
];
const SPINE_OFFSET_X: [f32; 12] = [
    0.0, 0.0, 8.0, -8.0, -8.0, 8.0, 8.0, -8.0, 0.0, 0.0, 8.0, -8.0,
];
const SPINE_OFFSET_Y: [f32; 12] = [
    -8.0, -8.0, -8.0, -8.0, 0.0, 0.0, 0.0, 0.0, 8.0, 8.0, 8.0, 8.0,
];
const SPINE_OFFSET_Z: [f32; 12] = [
    8.0, -8.0, 0.0, 0.0, -8.0, -8.0, 8.0, 8.0, 8.0, -8.0, 0.0, 0.0,
];

/// What a guardian's own state does to its model.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GuardianLook {
    /// The phase of the tail's swing (`EntityGuardian.func_175471_a`).
    pub tail: f32,
    /// How far the spikes are out, `0..=1` (`func_175469_o`): retracted
    /// while it swims.
    pub extension: f32,
    /// Where the pupil sits on the face, vanilla `(x, y)` pixels: towards
    /// the side and the height of what it watches.
    pub eye: [f32; 2],
}

/// Guardian model (64×64 texture sheet) with body, eye, 12 spines and 3-segment tail.
#[derive(Debug, Clone)]
pub struct GuardianModel {
    /// Main cubic torso and armor plates; the other parts hang on it.
    pub body: ModelPart,
    /// Center pupil/eye.
    pub eye: ModelPart,
    /// 12 retractable spines.
    pub spines: [ModelPart; 12],
    /// 3 tail segments, each hanging on the one before.
    pub tail: [ModelPart; 3],
    /// Scale multiplier (1.0 for Guardian, 2.35 for Elder Guardian).
    pub scale: f32,
}

/// A vanilla box `(x, y, z, w, h, d)` in the flipped frame.
fn vanilla_box(origin: [f32; 3], size: [f32; 3], uv: [f32; 2]) -> ModelBox {
    ModelBox::new(
        [origin[0], -(origin[1] + size[1]), origin[2]],
        size,
        uv,
        [64.0, 64.0],
        0.0,
    )
}

impl GuardianModel {
    /// Creates a normal Guardian model.
    #[must_use]
    pub fn new() -> Self {
        Self::with_scale(1.0)
    }

    /// Creates an Elder Guardian model (scale 2.35).
    #[must_use]
    pub fn elder() -> Self {
        Self::with_scale(ELDER_SCALE)
    }

    /// Creates a guardian model with custom scale.
    #[must_use]
    pub fn with_scale(scale: f32) -> Self {
        let body = ModelPart::new([0.0, ROOT_Y, 0.0])
            .with_box(vanilla_box(
                [-6.0, 10.0, -8.0],
                [12.0, 12.0, 16.0],
                [0.0, 0.0],
            ))
            .with_box(vanilla_box(
                [-8.0, 10.0, -6.0],
                [2.0, 12.0, 12.0],
                [0.0, 28.0],
            ))
            .with_box(vanilla_box([6.0, 10.0, -6.0], [2.0, 12.0, 12.0], [0.0, 28.0]).mirrored())
            .with_box(vanilla_box(
                [-6.0, 8.0, -6.0],
                [12.0, 2.0, 12.0],
                [16.0, 40.0],
            ))
            .with_box(vanilla_box(
                [-6.0, 22.0, -6.0],
                [12.0, 2.0, 12.0],
                [16.0, 40.0],
            ));
        let spines = std::array::from_fn(|_| {
            ModelPart::new([0.0; 3]).with_box(vanilla_box(
                [-1.0, -4.5, -1.0],
                [2.0, 9.0, 2.0],
                [0.0, 0.0],
            ))
        });
        let eye = ModelPart::new([0.0, 0.0, -8.25]).with_box(vanilla_box(
            [-1.0, 15.0, 0.0],
            [2.0, 2.0, 1.0],
            [8.0, 0.0],
        ));
        let tail = [
            ModelPart::new([0.0; 3]).with_box(vanilla_box(
                [-2.0, 14.0, 7.0],
                [4.0, 4.0, 8.0],
                [40.0, 0.0],
            )),
            ModelPart::new([-1.5, -0.5, 14.0]).with_box(vanilla_box(
                [0.0, 14.0, 0.0],
                [3.0, 3.0, 7.0],
                [0.0, 54.0],
            )),
            ModelPart::new([0.5, -0.5, 6.0])
                .with_box(vanilla_box([0.0, 14.0, 0.0], [2.0, 2.0, 6.0], [41.0, 32.0]))
                .with_box(vanilla_box([1.0, 10.5, 3.0], [1.0, 9.0, 9.0], [25.0, 19.0])),
        ];
        let mut model = Self {
            body,
            eye,
            spines,
            tail,
            scale,
        };
        model.set_angles(0.0, 0.0, 0.0, GuardianLook::default());
        model
    }

    /// Poses the guardian (MCP `ModelGuardian.setRotationAngles`): the whole
    /// body turns with the look (degrees), the spines breathe with `age`
    /// (ticks lived plus the partial tick) and slide in as the guardian
    /// swims, the tail swings and the pupil looks at what it watches.
    pub fn set_angles(&mut self, age: f32, head_yaw: f32, head_pitch: f32, look: GuardianLook) {
        self.body.rotation = [-head_pitch.to_radians(), head_yaw.to_radians(), 0.0];
        let retraction = (1.0 - look.extension) * 0.55;
        for (i, spine) in self.spines.iter_mut().enumerate() {
            spine.rotation = [
                -PI * SPINE_TURN_X[i],
                PI * SPINE_TURN_Y[i],
                -PI * SPINE_TURN_Z[i],
            ];
            let reach = 1.0 + (age * 1.5 + i as f32).cos() * 0.01 - retraction;
            spine.pivot = [
                SPINE_OFFSET_X[i] * reach,
                -(16.0 + SPINE_OFFSET_Y[i] * reach),
                SPINE_OFFSET_Z[i] * reach,
            ];
        }
        self.eye.pivot = [look.eye[0], -look.eye[1], -8.25];
        let swing = look.tail.sin() * PI;
        for (segment, share) in self.tail.iter_mut().zip([0.05, 0.1, 0.15]) {
            segment.rotation[1] = swing * share;
        }
    }

    /// Emits the guardian mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(self.scale / 16.0));
        let body = self.body.transform(root);
        self.body.emit(root, &mut out);
        self.eye.emit(body, &mut out);
        for spine in &self.spines {
            spine.emit(body, &mut out);
        }
        let mut parent = body;
        for segment in &self.tail {
            segment.emit(parent, &mut out);
            parent = segment.transform(parent);
        }
        out
    }
}

impl Default for GuardianModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowest_and_highest(model: &GuardianModel) -> (f32, f32) {
        model
            .emit_mesh()
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), vertex| {
                (low.min(vertex.position[1]), high.max(vertex.position[1]))
            })
    }

    #[test]
    fn the_body_sits_on_the_ground_like_the_reference() {
        // Regression: the rebuilt body floated 4 pixels too high. The vanilla
        // plates span y 8 to 24, so 0 to 16 here; the spines reach out 4.5
        // pixels beyond the top.
        let model = GuardianModel::new();
        let body: Vec<_> = {
            let mut out = Vec::new();
            model
                .body
                .emit(Mat4::from_scale(Vec3::splat(1.0 / 16.0)), &mut out);
            out
        };
        let low = body.iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        let high = body.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        assert!(
            low.abs() < 1e-6 && (high - 1.0).abs() < 1e-6,
            "{low} {high}"
        );
    }

    #[test]
    fn swimming_draws_the_spikes_in() {
        let mut model = GuardianModel::new();
        let out = GuardianLook {
            extension: 1.0,
            ..GuardianLook::default()
        };
        model.set_angles(0.0, 0.0, 0.0, out);
        let spread = model.spines[0].pivot[2].abs();
        model.set_angles(0.0, 0.0, 0.0, GuardianLook::default());
        assert!(model.spines[0].pivot[2].abs() < spread * 0.5);
    }

    #[test]
    fn the_tail_swings_and_the_body_turns_with_the_look() {
        let mut model = GuardianModel::new();
        let look = GuardianLook {
            tail: PI / 2.0,
            extension: 1.0,
            eye: [1.0, 1.0],
        };
        model.set_angles(0.0, 90.0, 0.0, look);
        assert!((model.tail[2].rotation[1] - PI * 0.15).abs() < 1e-6);
        assert!((model.body.rotation[1] - PI / 2.0).abs() < 1e-6);
        assert_eq!(model.eye.pivot, [1.0, -1.0, -8.25]);
    }

    #[test]
    fn an_elder_is_two_and_a_third_times_as_big() {
        let (low, high) = lowest_and_highest(&GuardianModel::new());
        let (elder_low, elder_high) = lowest_and_highest(&GuardianModel::elder());
        assert!(((elder_high - elder_low) / (high - low) - ELDER_SCALE).abs() < 1e-3);
    }
}
