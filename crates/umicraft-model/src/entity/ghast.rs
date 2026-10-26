// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ghast box model matching MCP 1.8 `ModelGhast.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Ghast model (64×32 texture sheet) with 1 large cubic body and 9 trailing tentacles.
#[derive(Debug, Clone)]
pub struct GhastModel {
    /// Large 16×16×16 body cube.
    pub body: ModelPart,
    /// 9 dangling animated tentacles.
    pub tentacles: [ModelPart; 9],
}

impl GhastModel {
    /// Creates a new ghast model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];

        // Body: 16×16×16, centered at Y = 24.0 (1.5 blocks above ground).
        let body = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-8.0, -8.0, -8.0],
            [16.0, 16.0, 16.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        // Deterministic tentacle lengths from MCP pseudo-random sequence with seed 1660L:
        let lengths: [f32; 9] = [11.0, 13.0, 9.0, 14.0, 10.0, 12.0, 8.0, 13.0, 11.0];

        let tentacles = std::array::from_fn(|j| {
            let col = j % 3;
            let row = j / 3;
            let offset_x = (col as f32 - (row % 2) as f32 * 0.5 + 0.25) - 0.75;
            let x = offset_x * 5.0;
            let z = (row as f32 - 1.0) * 5.0;
            let len = lengths[j];

            ModelPart::new([x, 16.0, z]).with_box(ModelBox::new(
                [-1.0, -len, -1.0],
                [2.0, len, 2.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
        });

        Self { body, tentacles }
    }

    /// Sets the tentacle sway animation angles.
    pub fn set_angles(&mut self, anim_time: f32) {
        for (j, tentacle) in self.tentacles.iter_mut().enumerate() {
            // Negative angle tilts tentacles backward (+Z in Umicraft)
            tentacle.rotation[0] = -0.2 * (anim_time * 0.3 + j as f32).sin() - 0.4;
        }
    }

    /// Emits the ghast mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.body.emit(root, &mut out);
        for tentacle in &self.tentacles {
            tentacle.emit(root, &mut out);
        }

        out
    }
}

impl Default for GhastModel {
    fn default() -> Self {
        Self::new()
    }
}
