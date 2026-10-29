// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baby proportions: the head keeps (almost) its size while the rest of the
//! body is halved, as the vanilla models draw a child (MCP
//! `ModelQuadruped.render`, `ModelBiped.render`, `ModelChicken.render`, ...).

use glam::{Mat4, Vec3};

use super::{ModelPart, ModelVertex};

/// How the head of a child sits on the halved body, in the units and the
/// y-down frame of the vanilla model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChildLayout {
    /// Scale of the head (`1.5 / 2` for bipeds and ocelots, else `1`).
    pub head_scale: f32,
    /// Shift of the head along the vanilla y axis (down), before the scale.
    pub head_down: f32,
    /// Shift of the head backwards, before the scale.
    pub head_back: f32,
}

impl ChildLayout {
    /// Cow, pig, sheep (`ModelQuadruped`: `childYOffset 8`, `childZOffset 4`).
    pub const QUADRUPED: Self = Self::new(1.0, 8.0, 4.0);
    /// Players and zombies (`ModelBiped`).
    pub const BIPED: Self = Self::new(0.75, 16.0, 0.0);
    /// Chicken, wolf and rabbit.
    pub const SMALL_ANIMAL: Self = Self::new(1.0, 5.0, 2.0);
    /// Ocelot (`ModelOcelot`).
    pub const OCELOT: Self = Self::new(0.75, 10.0, 4.0);

    const fn new(head_scale: f32, head_down: f32, head_back: f32) -> Self {
        Self {
            head_scale,
            head_down,
            head_back,
        }
    }

    /// The matrix, in model units with y up, that places the head: vanilla
    /// computes `scale * (v + shift)` in a frame whose y points down and
    /// whose feet are 24 units below the origin.
    fn head_matrix(self) -> Mat4 {
        let s = self.head_scale;
        Mat4::from_translation(Vec3::new(
            0.0,
            24.0 - s * (24.0 + self.head_down),
            s * self.head_back,
        )) * Mat4::from_scale(Vec3::splat(s))
    }
}

/// Emits a child: `heads` placed by `layout`, `bodies` halved about the
/// feet. Vertices are in blocks, like `emit_mesh`.
#[must_use]
pub fn emit_child(
    heads: &[&ModelPart],
    bodies: &[&ModelPart],
    layout: ChildLayout,
) -> Vec<ModelVertex> {
    let mut out = Vec::new();
    let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
    let head_parent = root * layout.head_matrix();
    for part in heads {
        part.emit(head_parent, &mut out);
    }
    let body_parent = root * Mat4::from_scale(Vec3::splat(0.5));
    for part in bodies {
        part.emit(body_parent, &mut out);
    }
    out
}

/// Scales a finished mesh about the feet, for the models that are simply
/// drawn smaller (villagers, foals).
pub fn scale_mesh(vertices: &mut [ModelVertex], factor: f32) {
    for vertex in vertices {
        vertex.position = vertex.position.map(|c| c * factor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{HumanoidModel, QuadrupedModel};

    fn top(vertices: &[ModelVertex]) -> f32 {
        vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn a_calf_is_half_as_tall_with_a_head_that_keeps_its_size() {
        let cow = QuadrupedModel::cow();
        let adult = cow.emit_mesh();
        let calf = cow.emit_child_mesh();
        assert_eq!(adult.len(), calf.len(), "same parts, new placement");
        assert!(
            top(&calf) < top(&adult) * 0.75,
            "{} {}",
            top(&calf),
            top(&adult)
        );
        assert!(
            calf.iter().all(|v| v.position[1] >= -0.01),
            "feet on the ground"
        );
    }

    #[test]
    fn a_baby_biped_has_a_head_of_three_quarters() {
        let baby = HumanoidModel::zombie().emit_child_mesh();
        let head_top = top(&baby);
        // The head (8 units) at 0.75 on top of a body of 24 / 2.
        assert!(
            (head_top - (12.0 + 8.0 * 0.75 - 0.0) / 16.0).abs() < 0.2,
            "{head_top}"
        );
    }

    #[test]
    fn scaling_a_mesh_keeps_the_feet_on_the_ground() {
        let mut mesh = QuadrupedModel::pig().emit_mesh();
        let before = top(&mesh);
        scale_mesh(&mut mesh, 0.5);
        assert!((top(&mesh) - before * 0.5).abs() < 1e-5);
    }
}
