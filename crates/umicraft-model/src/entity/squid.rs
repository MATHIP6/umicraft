// SPDX-License-Identifier: MIT OR Apache-2.0

//! Squid box model matching MCP 1.8 `ModelSquid.java` and the body transform
//! of `RenderSquid.rotateCorpse`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Height of the point the body turns about, in pixels
/// (`RenderSquid.rotateCorpse`: `translate(0, 0.5, 0)`).
const PIVOT_HEIGHT: f32 = 8.0;
/// How far the model hangs below that point, in pixels
/// (`RenderSquid.rotateCorpse`: `translate(0, -1.2, 0)`).
const HANG: f32 = 19.2;

/// Squid model (64×32 texture sheet) with body sac and 8 radial tentacles.
#[derive(Debug, Clone)]
pub struct SquidModel {
    /// Main body sac (12×16×12).
    pub body: ModelPart,
    /// 8 tentacles arranged in a circle.
    pub tentacles: [ModelPart; 8],
    /// Turn of the whole body about the horizontal axis, in degrees
    /// (MCP `squidPitch`).
    pub pitch: f32,
    /// Turn of the whole body about the vertical axis, in degrees
    /// (MCP `squidYaw`).
    pub yaw: f32,
}

impl SquidModel {
    /// Creates a new Squid model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];
        // MCP: body at y = 24 - 16 = 8, tentacles at y = 31 - 16 = 15, in a
        // frame where y points down. Here y points up from the entity's
        // feet, so a point is `24 - y`.
        let body = ModelPart::new([0.0, 24.0 - 8.0, 0.0]).with_box(ModelBox::new(
            [-6.0, -8.0, -6.0],
            [12.0, 16.0, 12.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let tentacles = std::array::from_fn(|j| {
            let angle = j as f32 * std::f32::consts::PI * 2.0 / 8.0;
            let mut tentacle =
                ModelPart::new([angle.cos() * 5.0, 24.0 - 15.0, angle.sin() * 5.0]).with_box(
                    ModelBox::new([-1.0, -18.0, -1.0], [2.0, 18.0, 2.0], [48.0, 0.0], tex, 0.0),
                );
            tentacle.rotation[1] = -angle + (std::f32::consts::PI / 2.0);
            tentacle
        });

        Self {
            body,
            tentacles,
            pitch: 0.0,
            yaw: 0.0,
        }
    }

    /// Sets the tentacle angle (MCP `ageInTicks`, the squid's `tentacleAngle`).
    pub fn set_angles(&mut self, tentacle_angle: f32) {
        for tentacle in &mut self.tentacles {
            // The model's y axis is flipped, which flips turns about x.
            tentacle.rotation[0] = -tentacle_angle;
        }
    }

    /// Sets the turn of the whole body, in degrees.
    pub fn set_pose(&mut self, pitch: f32, yaw: f32) {
        self.pitch = pitch;
        self.yaw = yaw;
    }

    /// Emits all mesh vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0))
            * Mat4::from_translation(Vec3::new(0.0, PIVOT_HEIGHT, 0.0))
            * Mat4::from_rotation_x(self.pitch.to_radians())
            * Mat4::from_rotation_y(self.yaw.to_radians())
            * Mat4::from_translation(Vec3::new(0.0, -HANG, 0.0));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(vertices: &[ModelVertex], axis: usize) -> (f32, f32) {
        vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), v| {
                (low.min(v.position[axis]), high.max(v.position[axis]))
            })
    }

    #[test]
    fn the_rest_pose_hangs_around_the_feet_not_above_them() {
        let (low, high) = extent(&SquidModel::new().emit_mesh(), 1);
        // Body 0.3 ± 0.5 above the feet, tentacles down to -1.2625 blocks.
        assert!((high - 0.8).abs() < 1e-3, "{high}");
        assert!((low + 1.2625).abs() < 1e-3, "{low}");
    }

    #[test]
    fn the_pitch_swings_the_tentacles_about_the_pivot() {
        let mut model = SquidModel::new();
        model.set_pose(90.0, 0.0);
        let vertices = model.emit_mesh();
        let (low_z, high_z) = extent(&vertices, 2);
        let (low_y, high_y) = extent(&vertices, 1);
        // Lying down, the long axis is horizontal and the body is flat.
        assert!(high_z - low_z > 1.5, "{}", high_z - low_z);
        assert!(high_y - low_y < 1.0, "{}", high_y - low_y);
    }
}
