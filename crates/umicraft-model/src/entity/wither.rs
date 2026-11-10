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
    /// The shell of a wounded wither: the model inflated by half a pixel (MCP
    /// `LayerWitherAura`, `new ModelWither(0.5F)`).
    #[must_use]
    pub fn aura() -> Self {
        let mut model = Self::new();
        for part in [
            &mut model.center_head,
            &mut model.right_head,
            &mut model.left_head,
            &mut model.shoulders,
            &mut model.spine,
            &mut model.tail,
        ] {
            part.grow(0.5);
        }
        model
    }

    /// Creates a new wither model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 64.0];

        // Center head: 8×8×8 at pivot [0, 24, 0], box [-4, -4, -4] (MCP ModelWither: field_82904_b[0])
        let center_head = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -4.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        // Right side head: 6×6×6 at pivot [-8, 20, 0], box [-4, -2, -4] (MCP ModelWither: field_82904_b[1])
        let right_head = ModelPart::new([-8.0, 20.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -2.0, -4.0],
            [6.0, 6.0, 6.0],
            [32.0, 0.0],
            tex,
            0.0,
        ));

        // Left side head: 6×6×6 at pivot [10, 20, 0], box [-4, -2, -4] (MCP ModelWither: field_82904_b[2])
        let left_head = ModelPart::new([10.0, 20.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -2.0, -4.0],
            [6.0, 6.0, 6.0],
            [32.0, 0.0],
            tex,
            0.0,
        ));

        // Shoulders bar: 20×3×3 (MCP: addBox(-10, 3.9, -0.5, 20, 3, 3))
        let shoulders = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-10.0, -6.9, -0.5],
            [20.0, 3.0, 3.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        // Spine and 3 ribs extending down from shoulders (MCP: setRotationPoint(-2, 6.9, -0.5))
        let spine = ModelPart::new([-2.0, 17.1, -0.5])
            .with_box(ModelBox::new(
                [0.0, -10.0, 0.0],
                [3.0, 10.0, 3.0],
                [0.0, 22.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -3.5, 0.5],
                [11.0, 2.0, 2.0],
                [24.0, 22.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -6.0, 0.5],
                [11.0, 2.0, 2.0],
                [24.0, 22.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -8.5, 0.5],
                [11.0, 2.0, 2.0],
                [24.0, 22.0],
                tex,
                0.0,
            ));

        // Tail spine (3×6×3) connected at bottom of spine
        let tail = ModelPart::new([-2.0, 7.1, -0.5]).with_box(ModelBox::new(
            [0.0, -6.0, 0.0],
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

    /// Turns the heads and breathes the spine (MCP
    /// `ModelWither.setRotationAngles`).
    ///
    /// `age` is the ticks lived plus the partial tick (MCP `ageInTicks`); the
    /// head angles are in degrees.
    pub fn set_angles(&mut self, age: f32, head_yaw: f32, head_pitch: f32) {
        self.center_head.rotation[1] = head_yaw.to_radians();
        self.center_head.rotation[0] = -head_pitch.to_radians();

        // The spine and tail breathe slowly with time.
        let sway = (age * 0.1).cos();
        let spine_angle = (0.065 + 0.05 * sway) * std::f32::consts::PI;
        self.spine.rotation[0] = -spine_angle;
        self.tail.pivot = [
            -2.0,
            17.1 - spine_angle.cos() * 10.0,
            -0.5 + spine_angle.sin() * 10.0,
        ];
        self.tail.rotation[0] = -(0.265 + 0.1 * sway) * std::f32::consts::PI;
    }

    /// Turns the two side heads (MCP `ModelWither.setLivingAnimations`): `yaw`
    /// relative to the torso and `pitch`, in degrees, right head first. The
    /// vanilla pitch is negated for the flipped y.
    pub fn set_side_heads(&mut self, yaw: [f32; 2], pitch: [f32; 2]) {
        for (head, (yaw, pitch)) in [&mut self.right_head, &mut self.left_head]
            .into_iter()
            .zip(yaw.into_iter().zip(pitch))
        {
            head.rotation[1] = yaw.to_radians();
            head.rotation[0] = -pitch.to_radians();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_side_heads_look_their_own_way() {
        let mut wither = WitherModel::new();
        wither.set_angles(0.0, 0.0, 0.0);
        wither.set_side_heads([30.0, -45.0], [10.0, 0.0]);
        assert!((wither.right_head.rotation[1] - 30.0_f32.to_radians()).abs() < 1e-6);
        assert!((wither.left_head.rotation[1] + 45.0_f32.to_radians()).abs() < 1e-6);
        assert!((wither.right_head.rotation[0] + 10.0_f32.to_radians()).abs() < 1e-6);
        assert_eq!(wither.center_head.rotation, [0.0; 3]);
    }
}
