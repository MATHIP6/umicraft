// SPDX-License-Identifier: MIT OR Apache-2.0

//! Squid box model matching MCP 1.8 `ModelSquid.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Squid model (64×32 texture sheet) with body sac and 8 radial tentacles.
#[derive(Debug, Clone)]
pub struct SquidModel {
    /// Main body sac (12×16×12).
    pub body: ModelPart,
    /// 8 tentacles arranged in a circle.
    pub tentacles: [ModelPart; 8],
}

impl SquidModel {
    /// Creates a new Squid model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        // In MCP: body at Y = 24 - 16 = 8.0, tentacles at Y = 31 - 16 = 15.0.
        // In Umicraft:
        let body_py = 18.0;
        let tentacle_py = 18.0;

        let body = ModelPart::new([0.0, body_py, 0.0]).with_box(ModelBox::new(
            [-6.0, 0.0, -6.0],
            [12.0, 16.0, 12.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let mut tentacles = [
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
            ModelPart::new([0.0, tentacle_py, 0.0]),
        ];

        for (j, tentacle) in tentacles.iter_mut().enumerate() {
            let angle = (j as f32) * std::f32::consts::PI * 2.0 / 8.0;
            let px = angle.cos() * 5.0;
            let pz = angle.sin() * 5.0;

            *tentacle = ModelPart::new([px, tentacle_py, pz]).with_box(ModelBox::new(
                [-1.0, -18.0, -1.0],
                [2.0, 18.0, 2.0],
                [48.0, 0.0],
                tex,
                0.0,
            ));
            tentacle.rotation[1] = -angle + (std::f32::consts::PI / 2.0);
        }

        Self { body, tentacles }
    }

    /// Sets tentacle sway animation angles.
    pub fn set_angles(&mut self, tentacle_angle: f32) {
        for tentacle in &mut self.tentacles {
            tentacle.rotation[0] = tentacle_angle;
        }
    }

    /// Emits all mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.body.emit(root, &mut out);
        for tentacle in &self.tentacles {
            tentacle.emit(root, &mut out);
        }
        out
    }
}

impl Default for SquidModel {
    fn default() -> Self {
        Self::new()
    }
}
