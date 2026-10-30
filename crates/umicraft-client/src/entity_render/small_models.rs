// SPDX-License-Identifier: MIT OR Apache-2.0

//! Two objects drawn from a single box: the flying wither skull
//! (MCP `RenderWitherSkull`, `ModelSkeletonHead`) and the knot of a leash on a
//! fence (MCP `RenderLeashKnot`, `ModelLeashKnot`).
//!
//! Both renderers draw through `scale(-1, -1, 1)` with the model at the
//! entity, which this crate's models (y flipped) show as a mirror along x.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{ModelBox, ModelPart, TextureSlot};
use umicraft_render::EntityVertex;

use super::transform::x_mirror;

/// Adds the wither skull or the leash knot `ent` to `batches` when it is one.
pub(super) fn add_small_model(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    light: u32,
) {
    // `ModelSkeletonHead(0, 0, 64, 64)`: the head box turned by the yaw and the
    // pitch of the entity. `ModelLeashKnot`: a 6 x 8 x 6 box, not turned.
    let (slot, part) = match ent.kind {
        EntityKind::WitherSkull => {
            let mut part = ModelPart::new([0.0; 3]).with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                [64.0, 64.0],
                0.0,
            ));
            part.rotation = [-ent.pitch.to_radians(), ent.yaw.to_radians(), 0.0];
            (TextureSlot::Wither, part)
        }
        EntityKind::LeashKnot => (
            TextureSlot::LeashKnot,
            ModelPart::new([0.0; 3]).with_box(ModelBox::new(
                [-3.0, -2.0, -3.0],
                [6.0, 8.0, 6.0],
                [0.0, 0.0],
                [32.0, 32.0],
                0.0,
            )),
        ),
        _ => return,
    };
    let placed = Mat4::from_translation(ent.position) * x_mirror();
    let mut vertices = Vec::new();
    part.emit(Mat4::from_scale(Vec3::splat(1.0 / 16.0)), &mut vertices);
    batches
        .entry(slot)
        .or_default()
        .extend(vertices.into_iter().map(|vertex| {
            EntityVertex {
                position: placed
                    .transform_point3(Vec3::from(vertex.position))
                    .to_array(),
                normal: placed
                    .transform_vector3(Vec3::from(vertex.normal))
                    .normalize_or_zero()
                    .to_array(),
                uv: vertex.uv,
                light,
            }
        }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn model(kind: EntityKind, yaw: f32) -> Vec<EntityVertex> {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(3.0, 70.0, 3.0));
        ent.yaw = yaw;
        let mut batches = HashMap::new();
        add_small_model(&mut batches, &ent, 0xFF);
        batches.into_values().next().unwrap_or_default()
    }

    fn range(vertices: &[EntityVertex], axis: usize) -> (f32, f32) {
        let values: Vec<f32> = vertices.iter().map(|v| v.position[axis]).collect();
        (
            values.iter().cloned().fold(f32::MAX, f32::min),
            values.iter().cloned().fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn a_wither_skull_is_an_eight_pixel_cube_standing_on_its_position() {
        let skull = model(EntityKind::WitherSkull, 0.0);
        assert_eq!(skull.len(), 36);
        let (low, high) = range(&skull, 1);
        assert!((low - 70.0).abs() < 1e-4 && (high - 70.5).abs() < 1e-4);
        let (x0, x1) = range(&skull, 0);
        assert!((x1 - x0 - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_wither_skull_turns_with_its_yaw() {
        // An eighth of a turn widens the cube along x to its diagonal.
        let (x0, x1) = range(&model(EntityKind::WitherSkull, 45.0), 0);
        assert!(x1 - x0 > 0.65, "{}", x1 - x0);
    }

    #[test]
    fn a_leash_knot_is_six_by_eight_by_six_around_its_position() {
        let knot = model(EntityKind::LeashKnot, 0.0);
        assert_eq!(knot.len(), 36);
        // `addBox(-3, -6, -3, 6, 8, 6)` under `scale(-1, -1, 1)`: from 2
        // pixels under the position to 6 over it.
        let (low, high) = range(&knot, 1);
        assert!((low - (70.0 - 2.0 / 16.0)).abs() < 1e-4, "{low}");
        assert!((high - (70.0 + 6.0 / 16.0)).abs() < 1e-4, "{high}");
        let (z0, z1) = range(&knot, 2);
        assert!((z1 - z0 - 6.0 / 16.0).abs() < 1e-4);
    }

    #[test]
    fn other_kinds_draw_nothing() {
        assert!(model(EntityKind::Pig, 0.0).is_empty());
    }
}
