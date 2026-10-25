// SPDX-License-Identifier: MIT OR Apache-2.0

//! Silverfish box model matching MCP 1.8 `ModelSilverfish.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Silverfish model (64×32 texture sheet) with 7 undulating body segments and 3 carapace plates.
#[derive(Debug, Clone)]
pub struct SilverfishModel {
    /// 7 body segments along the spine.
    pub body_parts: [ModelPart; 7],
    /// 3 dorsal wing/carapace plates.
    pub wings: [ModelPart; 3],
}

impl SilverfishModel {
    /// Creates a new silverfish model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        let box_sizes: [[f32; 3]; 7] = [
            [3.0, 2.0, 2.0],
            [4.0, 3.0, 2.0],
            [6.0, 4.0, 3.0],
            [3.0, 3.0, 3.0],
            [2.0, 2.0, 3.0],
            [2.0, 1.0, 2.0],
            [1.0, 1.0, 2.0],
        ];
        let tex_uvs: [[f32; 2]; 7] = [
            [0.0, 0.0],
            [0.0, 4.0],
            [0.0, 9.0],
            [0.0, 16.0],
            [0.0, 22.0],
            [11.0, 0.0],
            [13.0, 4.0],
        ];

        let mut body_parts: [ModelPart; 7] =
            std::array::from_fn(|_| ModelPart::new([0.0, 0.0, 0.0]));
        let mut z_pos = -3.5_f32;
        let mut z_positions = [0.0_f32; 7];

        for i in 0..7 {
            let sz = box_sizes[i];
            let uv = tex_uvs[i];
            let part = ModelPart::new([0.0, 0.0, z_pos]).with_box(ModelBox::new(
                [-sz[0] * 0.5, 0.0, -sz[2] * 0.5],
                sz,
                uv,
                tex,
                0.0,
            ));
            z_positions[i] = z_pos;
            if i < 6 {
                z_pos += (sz[2] + box_sizes[i + 1][2]) * 0.5;
            }
            body_parts[i] = part;
        }

        let wing0 = ModelPart::new([0.0, 0.0, z_positions[2]]).with_box(ModelBox::new(
            [-5.0, 0.0, -box_sizes[2][2] * 0.5],
            [10.0, 8.0, box_sizes[2][2]],
            [20.0, 0.0],
            tex,
            0.0,
        ));
        let wing1 = ModelPart::new([0.0, 0.0, z_positions[4]]).with_box(ModelBox::new(
            [-3.0, 0.0, -box_sizes[4][2] * 0.5],
            [6.0, 4.0, box_sizes[4][2]],
            [20.0, 11.0],
            tex,
            0.0,
        ));
        let wing2 = ModelPart::new([0.0, 0.0, z_positions[1]]).with_box(ModelBox::new(
            [-3.0, 0.0, -box_sizes[1][2] * 0.5],
            [6.0, 5.0, box_sizes[1][2]],
            [20.0, 18.0],
            tex,
            0.0,
        ));

        Self {
            body_parts,
            wings: [wing0, wing1, wing2],
        }
    }

    /// Sets the undulating slither animation.
    pub fn set_angles(&mut self, anim_time: f32) {
        for (i, part) in self.body_parts.iter_mut().enumerate() {
            let offset = anim_time * 0.9 + (i as f32) * 0.15 * std::f32::consts::PI;
            let weight = (1 + (i as i32 - 2).abs()) as f32;
            part.rotation[1] = offset.cos() * std::f32::consts::PI * 0.05 * weight;
            part.pivot[0] = offset.sin() * std::f32::consts::PI * 0.2 * (i as i32 - 2).abs() as f32;
        }

        self.wings[0].rotation[1] = self.body_parts[2].rotation[1];
        self.wings[1].rotation[1] = self.body_parts[4].rotation[1];
        self.wings[1].pivot[0] = self.body_parts[4].pivot[0];
        self.wings[2].rotation[1] = self.body_parts[1].rotation[1];
        self.wings[2].pivot[0] = self.body_parts[1].pivot[0];
    }

    /// Emits the silverfish mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        for part in &self.body_parts {
            part.emit(root, &mut out);
        }
        for wing in &self.wings {
            wing.emit(root, &mut out);
        }

        out
    }
}

impl Default for SilverfishModel {
    fn default() -> Self {
        Self::new()
    }
}
