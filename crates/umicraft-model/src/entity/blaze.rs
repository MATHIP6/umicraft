// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blaze box model matching MCP 1.8 `ModelBlaze.java`.

use std::f32::consts::PI;

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

        // MCP pivots are y-down from the top of the legs; here y is up from
        // the feet, so an MCP `y` becomes `24 - y` and boxes hang below.
        let head = ModelPart::new([0.0, 24.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -4.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let sticks = std::array::from_fn(|_| {
            ModelPart::new([0.0, 0.0, 0.0]).with_box(ModelBox::new(
                [0.0, -8.0, 0.0],
                [2.0, 8.0, 2.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
        });

        Self { head, sticks }
    }

    /// Turns the head and moves the 12 rods along their orbits
    /// (MCP `ModelBlaze.setRotationAngles`).
    ///
    /// `age` is the ticks lived plus the partial tick (MCP `ageInTicks`): a
    /// blaze hovers, so its rods turn with time, not with walking. The head
    /// angles are in degrees.
    pub fn set_angles(&mut self, age: f32, head_yaw: f32, head_pitch: f32) {
        self.head.rotation[1] = head_yaw.to_radians();
        // Flipping y turns the sense of x rotations around (as in `pose.rs`).
        self.head.rotation[0] = -head_pitch.to_radians();

        let (top, rest) = self.sticks.split_at_mut(4);
        let (middle, bottom) = rest.split_at_mut(4);
        // Each tier turns at its own speed; each rod bobs with its index `k`
        // (`0..12` over the three tiers) so the rods drift out of phase.
        orbit(top, 0, 9.0, age * PI * -0.1, |k| {
            -2.0 + (((k * 2) as f32 + age) * 0.25).cos()
        });
        orbit(middle, 4, 7.0, PI / 4.0 + age * PI * 0.03, |k| {
            2.0 + (((k * 2) as f32 + age) * 0.25).cos()
        });
        orbit(bottom, 8, 5.0, 0.471_238_94 + age * PI * -0.05, |k| {
            11.0 + ((k as f32 * 1.5 + age) * 0.5).cos()
        });
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

/// Places a tier of rods evenly (one radian apart) on a circle of `radius`
/// from angle `start`, each at the MCP height `mcp_y(k)` of rod `k`, `first`
/// being the index of the first rod of the tier.
fn orbit(
    sticks: &mut [ModelPart],
    first: usize,
    radius: f32,
    start: f32,
    mcp_y: impl Fn(usize) -> f32,
) {
    for (i, stick) in sticks.iter_mut().enumerate() {
        let angle = start + i as f32;
        let k = first + i;
        stick.pivot = [angle.cos() * radius, 24.0 - mcp_y(k), angle.sin() * radius];
    }
}

impl Default for BlazeModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rods_orbit_with_age_alone() {
        // Regression: the rods were driven by the limb swing, which stays
        // still while a blaze hovers, so they never moved.
        let mut early = BlazeModel::new();
        early.set_angles(0.0, 0.0, 0.0);
        let mut later = BlazeModel::new();
        later.set_angles(5.0, 0.0, 0.0);
        for (a, b) in early.sticks.iter().zip(&later.sticks) {
            assert_ne!(a.pivot, b.pivot);
        }
    }

    #[test]
    fn the_rods_sit_at_the_mcp_heights() {
        // MCP `ModelBlaze`: at age 0 rod `k` of the top tier is at
        // `-2 + cos(k * 2 * 0.25)`, which is `24 - y` here.
        let mut model = BlazeModel::new();
        model.set_angles(0.0, 0.0, 0.0);
        let expected = [
            24.0 - (-2.0 + 0.0_f32.cos()),
            24.0 - (2.0 + (8.0_f32 * 0.25).cos()),
            24.0 - (11.0 + (8.0_f32 * 1.5 * 0.5).cos()),
        ];
        for (stick, y) in [0, 4, 8].into_iter().zip(expected) {
            assert!((model.sticks[stick].pivot[1] - y).abs() < 1e-5);
        }
        let radius = model.sticks[0].pivot[0].hypot(model.sticks[0].pivot[2]);
        assert!((radius - 9.0).abs() < 1e-5);
    }

    #[test]
    fn the_head_turns_by_degrees() {
        let mut model = BlazeModel::new();
        model.set_angles(0.0, 90.0, -30.0);
        assert!((model.head.rotation[1] - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((model.head.rotation[0] - 30.0_f32.to_radians()).abs() < 1e-5);
    }
}
