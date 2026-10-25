// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blaze box model matching MCP 1.8 `ModelBlaze.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Blaze model (64×32 texture sheet) with floating rotating rods.
#[derive(Debug, Clone)]
pub struct BlazeModel {
    /// Floating head part.
    pub head: ModelPart,
    /// 12 floating blaze rods arranged in three orbiting tiers.
    pub sticks: [ModelPart; 12],
}

impl BlazeModel {
    /// Creates a new blaze model with 12 rods.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];

        let head = ModelPart::new([0.0, 18.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -4.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let sticks = std::array::from_fn(|_| {
            ModelPart::new([0.0, 0.0, 0.0]).with_box(ModelBox::new(
                [-1.0, -4.0, -1.0],
                [2.0, 8.0, 2.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
        });

        Self { head, sticks }
    }

    /// Sets the rotation and orbiting positions for all 12 sticks.
    pub fn set_angles(&mut self, anim_time: f32, head_yaw: f32, head_pitch: f32) {
        self.head.rotation[1] = head_yaw;
        self.head.rotation[0] = head_pitch;

        // Tier 1: sticks 0..4 (radius 9.0)
        let mut angle1 = anim_time * std::f32::consts::PI * -0.1;
        for (i, stick) in self.sticks[0..4].iter_mut().enumerate() {
            let y_off = ((i * 2) as f32 + anim_time * 0.25).cos();
            stick.pivot = [angle1.cos() * 9.0, 20.0 + y_off, angle1.sin() * 9.0];
            angle1 += 1.0;
        }

        // Tier 2: sticks 4..8 (radius 7.0)
        let mut angle2 = (std::f32::consts::PI / 4.0) + anim_time * std::f32::consts::PI * 0.03;
        for (i, stick) in self.sticks[4..8].iter_mut().enumerate() {
            let y_off = (((i + 4) * 2) as f32 + anim_time * 0.25).cos();
            stick.pivot = [angle2.cos() * 7.0, 14.0 + y_off, angle2.sin() * 7.0];
            angle2 += 1.0;
        }

        // Tier 3: sticks 8..12 (radius 5.0)
        let mut angle3 = 0.471_238_94 + anim_time * std::f32::consts::PI * -0.05;
        for (i, stick) in self.sticks[8..12].iter_mut().enumerate() {
            let k = (i + 8) as f32;
            let y_off = (k * 1.5 + anim_time * 0.5).cos();
            stick.pivot = [angle3.cos() * 5.0, 6.0 + y_off, angle3.sin() * 5.0];
            angle3 += 1.0;
        }
    }

    /// Emits the blaze mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.head.emit(root, &mut out);
        for stick in &self.sticks {
            stick.emit(root, &mut out);
        }

        out
    }
}

impl Default for BlazeModel {
    fn default() -> Self {
        Self::new()
    }
}
