// SPDX-License-Identifier: MIT OR Apache-2.0

//! Guardian and Elder Guardian box models matching MCP 1.8 `ModelGuardian.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Guardian model (64×64 texture sheet) with body, eye, 12 spines and 3-segment tail.
#[derive(Debug, Clone)]
pub struct GuardianModel {
    /// Main cubic torso and armor plates.
    pub body: ModelPart,
    /// Center pupil/eye.
    pub eye: ModelPart,
    /// 12 retractable spines.
    pub spines: [ModelPart; 12],
    /// 3 tail segments.
    pub tail: [ModelPart; 3],
    /// Scale multiplier (1.0 for Guardian, 2.35 for Elder Guardian).
    pub scale: f32,
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
        Self::with_scale(2.35)
    }

    /// Creates a guardian model with custom scale.
    #[must_use]
    pub fn with_scale(scale: f32) -> Self {
        let tex = [64.0, 64.0];

        // Body centered at Y = 12.0
        let body = ModelPart::new([0.0, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-6.0, -6.0, -8.0],
                [12.0, 12.0, 16.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-8.0, -6.0, -6.0],
                [2.0, 12.0, 12.0],
                [0.0, 28.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [6.0, -6.0, -6.0],
                [2.0, 12.0, 12.0],
                [0.0, 28.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-6.0, 6.0, -6.0],
                [12.0, 2.0, 12.0],
                [16.0, 40.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-6.0, -8.0, -6.0],
                [12.0, 2.0, 12.0],
                [16.0, 40.0],
                tex,
                0.0,
            ));

        // Eye: (-1, -1, -8.25, 2, 2, 1)
        let eye = ModelPart::new([0.0, 12.0, -8.25]).with_box(ModelBox::new(
            [-1.0, -1.0, 0.0],
            [2.0, 2.0, 1.0],
            [8.0, 0.0],
            tex,
            0.0,
        ));

        // 12 Spines
        let spines = std::array::from_fn(|_| {
            ModelPart::new([0.0, 0.0, 0.0]).with_box(ModelBox::new(
                [-1.0, -4.5, -1.0],
                [2.0, 9.0, 2.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
        });

        // Tail segments
        let tail0 = ModelPart::new([0.0, 12.0, 8.0]).with_box(ModelBox::new(
            [-2.0, -2.0, 0.0],
            [4.0, 4.0, 8.0],
            [40.0, 0.0],
            tex,
            0.0,
        ));

        let tail1 = ModelPart::new([0.0, 12.0, 16.0]).with_box(ModelBox::new(
            [-1.5, -1.5, 0.0],
            [3.0, 3.0, 7.0],
            [0.0, 54.0],
            tex,
            0.0,
        ));

        let tail2 = ModelPart::new([0.0, 12.0, 23.0])
            .with_box(ModelBox::new(
                [-1.0, -1.0, 0.0],
                [2.0, 2.0, 6.0],
                [41.0, 32.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-0.5, -4.5, 3.0],
                [1.0, 9.0, 9.0],
                [25.0, 19.0],
                tex,
                0.0,
            ));

        Self {
            body,
            eye,
            spines,
            tail: [tail0, tail1, tail2],
            scale,
        }
    }

    /// Sets spine pulsing and tail swimming animation angles.
    pub fn set_angles(&mut self, anim_time: f32) {
        let rot_x = [
            1.75, 0.25, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.25, 0.75, 0.0, 0.0,
        ];
        let rot_y = [
            0.0, 0.0, 0.0, 0.0, 0.25, 1.75, 1.25, 0.75, 0.0, 0.0, 0.0, 0.0,
        ];
        let rot_z = [
            0.0, 0.0, 0.25, 1.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, 1.25,
        ];
        let off_x = [
            0.0, 0.0, 8.0, -8.0, -8.0, 8.0, 8.0, -8.0, 0.0, 0.0, 8.0, -8.0,
        ];
        let off_y = [
            -8.0, -8.0, -8.0, -8.0, 0.0, 0.0, 0.0, 0.0, 8.0, 8.0, 8.0, 8.0,
        ];
        let off_z = [
            8.0, -8.0, 0.0, 0.0, -8.0, -8.0, 8.0, 8.0, 8.0, -8.0, 0.0, 0.0,
        ];

        let pi = std::f32::consts::PI;
        for i in 0..12 {
            self.spines[i].rotation[0] = pi * rot_x[i];
            self.spines[i].rotation[1] = pi * rot_y[i];
            self.spines[i].rotation[2] = pi * rot_z[i];
            let pulse = 1.0 + (anim_time * 1.5 + i as f32).cos() * 0.05;
            self.spines[i].pivot = [off_x[i] * pulse, 12.0 + off_y[i] * pulse, off_z[i] * pulse];
        }

        // Tail swim waggle
        let swim = (anim_time * 2.0).sin() * 0.2;
        self.tail[0].rotation[1] = swim;
        self.tail[1].rotation[1] = swim * 1.2;
        self.tail[2].rotation[1] = swim * 1.5;
    }

    /// Emits the guardian mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = (1.0 / 16.0) * self.scale;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.body.emit(root, &mut out);
        self.eye.emit(root, &mut out);
        for spine in &self.spines {
            spine.emit(root, &mut out);
        }
        for seg in &self.tail {
            seg.emit(root, &mut out);
        }

        out
    }
}

impl Default for GuardianModel {
    fn default() -> Self {
        Self::new()
    }
}
