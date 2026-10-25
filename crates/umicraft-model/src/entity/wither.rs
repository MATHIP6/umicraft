// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wither boss box model matching MCP 1.8 `ModelWither.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Wither boss model (64×64 texture sheet) with 3 heads, shoulder bar, ribbed spine and floating tail.
#[derive(Debug, Clone)]
pub struct WitherModel {
    /// Center main head.
    pub center_head: ModelPart,
    /// Right secondary head.
    pub right_head: ModelPart,
    /// Left secondary head.
    pub left_head: ModelPart,
    /// Horizontal shoulder bar.
    pub shoulders: ModelPart,
    /// Ribbed middle spine section.
    pub spine: ModelPart,
    /// Lower trailing tail.
    pub tail: ModelPart,
}

impl WitherModel {
    /// Creates a new wither model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 64.0];

        // Center head: 8×8×8
        let center_head = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        // Right side head: 6×6×6
        let right_head = ModelPart::new([-8.0, 20.0, 0.0]).with_box(ModelBox::new(
            [-3.0, 0.0, -3.0],
            [6.0, 6.0, 6.0],
            [32.0, 0.0],
            tex,
            0.0,
        ));

        // Left side head: 6×6×6
        let left_head = ModelPart::new([8.0, 20.0, 0.0]).with_box(ModelBox::new(
            [-3.0, 0.0, -3.0],
            [6.0, 6.0, 6.0],
            [32.0, 0.0],
            tex,
            0.0,
        ));

        // Shoulders bar: 20×3×3
        let shoulders = ModelPart::new([0.0, 20.0, 0.0]).with_box(ModelBox::new(
            [-10.0, 0.0, -1.5],
            [20.0, 3.0, 3.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        // Spine and 3 ribs
        let spine = ModelPart::new([0.0, 10.0, 0.0])
            .with_box(ModelBox::new(
                [-1.5, 0.0, -1.5],
                [3.0, 10.0, 3.0],
                [0.0, 22.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.5, 7.0, -1.0],
                [11.0, 2.0, 2.0],
                [24.0, 22.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.5, 4.5, -1.0],
                [11.0, 2.0, 2.0],
                [24.0, 22.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.5, 2.0, -1.0],
                [11.0, 2.0, 2.0],
                [24.0, 22.0],
                tex,
                0.0,
            ));

        // Tail spine (3×6×3)
        let tail = ModelPart::new([0.0, 4.0, 0.0]).with_box(ModelBox::new(
            [-1.5, 0.0, -1.5],
            [3.0, 6.0, 3.0],
            [12.0, 22.0],
            tex,
            0.0,
        ));

        Self {
            center_head,
            right_head,
            left_head,
            shoulders,
            spine,
            tail,
        }
    }

    /// Sets head rotations and hover spine undulation.
    pub fn set_angles(&mut self, anim_time: f32, head_yaw: f32, head_pitch: f32) {
        self.center_head.rotation[1] = head_yaw;
        self.center_head.rotation[0] = head_pitch;

        // Side heads look around slightly
        let sway = (anim_time * 0.1).cos();
        self.right_head.rotation[1] = head_yaw + (anim_time * 0.5).sin() * 0.2;
        self.left_head.rotation[1] = head_yaw + (anim_time * 0.5 + 1.0).sin() * 0.2;

        self.spine.rotation[0] = (0.065 + 0.05 * sway) * std::f32::consts::PI;
        self.tail.rotation[0] = (0.265 + 0.1 * sway) * std::f32::consts::PI;
    }

    /// Emits the wither mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.center_head.emit(root, &mut out);
        self.right_head.emit(root, &mut out);
        self.left_head.emit(root, &mut out);
        self.shoulders.emit(root, &mut out);
        self.spine.emit(root, &mut out);
        self.tail.emit(root, &mut out);

        out
    }
}

impl Default for WitherModel {
    fn default() -> Self {
        Self::new()
    }
}
