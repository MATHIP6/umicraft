// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endermite box model matching MCP 1.8 `ModelEnderMite.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Endermite model (64×32 texture sheet) with 4 writhing body segments.
#[derive(Debug, Clone)]
pub struct EndermiteModel {
    /// 4 body segments along the spine.
    pub body_parts: [ModelPart; 4],
}

impl EndermiteModel {
    /// Creates a new endermite model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        let box_sizes: [[f32; 3]; 4] = [
            [4.0, 3.0, 2.0],
            [6.0, 4.0, 5.0],
            [3.0, 3.0, 1.0],
            [1.0, 2.0, 1.0],
        ];
        let tex_uvs: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 5.0], [0.0, 14.0], [0.0, 18.0]];

        let mut body_parts: [ModelPart; 4] =
            std::array::from_fn(|_| ModelPart::new([0.0, 0.0, 0.0]));
        let mut z_pos = -3.5_f32;

        for i in 0..4 {
            let sz = box_sizes[i];
            let uv = tex_uvs[i];
            let part = ModelPart::new([0.0, 0.0, z_pos]).with_box(ModelBox::new(
                [-sz[0] * 0.5, 0.0, -sz[2] * 0.5],
                sz,
                uv,
                tex,
                0.0,
            ));
            if i < 3 {
                z_pos += (sz[2] + box_sizes[i + 1][2]) * 0.5;
            }
            body_parts[i] = part;
        }

        Self { body_parts }
    }

    /// Sets the writhing animation angles.
    pub fn set_angles(&mut self, anim_time: f32) {
        for (i, part) in self.body_parts.iter_mut().enumerate() {
            let offset = anim_time * 0.9 + (i as f32) * 0.15 * std::f32::consts::PI;
            let weight = (1 + (i as i32 - 2).abs()) as f32;
            part.rotation[1] = offset.cos() * std::f32::consts::PI * 0.01 * weight;
            part.pivot[0] = offset.sin() * std::f32::consts::PI * 0.1 * (i as i32 - 2).abs() as f32;
        }
    }

    /// Emits the endermite mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        for part in &self.body_parts {
            part.emit(root, &mut out);
        }

        out
    }
}

impl Default for EndermiteModel {
    fn default() -> Self {
        Self::new()
    }
}
