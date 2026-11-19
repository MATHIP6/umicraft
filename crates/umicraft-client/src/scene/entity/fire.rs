// SPDX-License-Identifier: MIT OR Apache-2.0

//! The flames on a burning entity (MCP `Render.renderEntityOnFire`).
//!
//! A stack of upright quads that face the camera, alternating the two fire
//! sprites of the block atlas, starting at the feet and climbing in steps of
//! 0.45 block while shrinking by a tenth each step.

use std::collections::HashMap;

use glam::Vec3;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// Frames in a fire sprite strip (`fire_layer_0.png` is 16 x 512).
pub(super) const FIRE_FRAMES: f32 = 32.0;
/// Full block and sky light: the fire does not depend on the light.
const FIRE_LIGHT: u32 = 0x0000_00FF;

/// Adds the flames of `ent` to `batches` when it is burning. `view_yaw` is
/// the yaw of the camera, in the convention of `umicraft_render::Camera`.
pub(super) fn add_fire(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    view_yaw: f32,
) {
    if !ent.flags.on_fire() {
        return;
    }
    let forward = Vec3::new(view_yaw.sin(), 0.0, -view_yaw.cos());
    let right = Vec3::new(view_yaw.cos(), 0.0, view_yaw.sin());
    // Frame of the strip: one per tick, as the atlas animation advances.
    let frame = (ent.age.floor().max(0.0) % FIRE_FRAMES).floor();
    let (v_min, v_max) = (frame / FIRE_FRAMES, (frame + 1.0) / FIRE_FRAMES);

    let size = ent.kind.width() * 1.4;
    let mut remaining = ent.kind.height() / size;
    let mut rise = 0.0_f32;
    let mut half = 0.5_f32;
    let mut depth = -0.3 + (remaining as i32) as f32 * 0.02;
    let mut index = 0u32;
    while remaining > 0.0 {
        let slot = TextureSlot::Fire((index % 2) as u8);
        let (mut u_left, mut u_right) = (1.0, 0.0);
        if (index / 2).is_multiple_of(2) {
            (u_left, u_right) = (u_right, u_left);
        }
        // After `rotate(-viewY)` local +z points away from the camera and +x to
        // the left of the view: the quads start in front of the entity and step
        // away from the camera.
        let at = |x: f32, y: f32| {
            ent.position + (forward * depth - right * x) * size + Vec3::Y * ((y + rise) * size)
        };
        let corners = [
            (at(half, 0.0), [u_left, v_max]),
            (at(-half, 0.0), [u_right, v_max]),
            (at(-half, 1.4), [u_right, v_min]),
            (at(half, 1.4), [u_left, v_min]),
        ];
        let batch = batches.entry(slot).or_default();
        for corner in [0, 1, 2, 0, 2, 3] {
            let (position, uv) = corners[corner];
            batch.push(EntityVertex {
                position: position.to_array(),
                normal: [0.0, 1.0, 0.0],
                uv,
                light: FIRE_LIGHT,
            });
        }
        remaining -= 0.45;
        rise += 0.45;
        half *= 0.9;
        depth += 0.03;
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId, EntityKind};

    fn burning(kind: EntityKind) -> EntityRenderSnapshot {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(5.0, 64.0, 5.0));
        ent.flags = EntityFlags::new(0x01, 0x7F);
        ent
    }

    fn fire(ent: &EntityRenderSnapshot, view_yaw: f32) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let mut batches = HashMap::new();
        add_fire(&mut batches, ent, view_yaw);
        batches
    }

    #[test]
    fn an_entity_that_is_not_burning_has_no_flames() {
        let ent = EntityRenderSnapshot::new(EntityId(1), EntityKind::Zombie, Vec3::ZERO);
        assert!(fire(&ent, 0.0).is_empty());
    }

    #[test]
    fn a_zombie_gets_six_quads_alternating_the_two_sprites() {
        // height 1.95 / (0.6 * 1.4) = 2.32 sprite heights: 6 steps of 0.45.
        let batches = fire(&burning(EntityKind::Zombie), 0.0);
        assert_eq!(batches[&TextureSlot::Fire(0)].len(), 3 * 6);
        assert_eq!(batches[&TextureSlot::Fire(1)].len(), 3 * 6);
    }

    #[test]
    fn the_flames_scale_with_the_width_and_climb_with_the_height() {
        let extent = |kind, axis: usize| -> f32 {
            let positions: Vec<f32> = fire(&burning(kind), 0.0)
                .values()
                .flatten()
                .map(|v| v.position[axis])
                .collect();
            positions.iter().cloned().fold(f32::MIN, f32::max)
                - positions.iter().cloned().fold(f32::MAX, f32::min)
        };
        let quads = |kind| -> usize { fire(&burning(kind), 0.0).values().map(Vec::len).sum() };
        assert!(quads(EntityKind::Zombie) > quads(EntityKind::Rabbit));
        // A giant is six times as wide: bigger flames, one step fewer (5 against 6).
        assert!(extent(EntityKind::Giant, 1) > 5.0 * extent(EntityKind::Zombie, 1));
        assert!(quads(EntityKind::Giant) < quads(EntityKind::Zombie));
    }

    #[test]
    fn the_quads_face_the_camera_and_start_at_the_feet() {
        for view_yaw in [0.0, 1.0, std::f32::consts::PI] {
            let ent = burning(EntityKind::Zombie);
            let batches = fire(&ent, view_yaw);
            let forward = Vec3::new(view_yaw.sin(), 0.0, -view_yaw.cos());
            for vertices in batches.values() {
                for quad in vertices.chunks(6) {
                    // Every vertex of a quad is at the same depth along the view.
                    let depth = |v: &EntityVertex| Vec3::from(v.position).dot(forward);
                    assert!(quad
                        .iter()
                        .all(|v| (depth(v) - depth(&quad[0])).abs() < 1e-4));
                }
            }
            let lowest = batches[&TextureSlot::Fire(0)]
                .iter()
                .map(|v| v.position[1])
                .fold(f32::MAX, f32::min);
            assert!((lowest - 64.0).abs() < 1e-4, "{lowest}");
            let highest = batches
                .values()
                .flatten()
                .map(|v| v.position[1])
                .fold(f32::MIN, f32::max);
            assert!(
                highest > 64.0 + 1.95,
                "the flames climb past the head: {highest}"
            );
        }
    }

    #[test]
    fn the_uv_picks_one_frame_of_the_strip() {
        let mut ent = burning(EntityKind::Zombie);
        ent.age = 5.4;
        let batches = fire(&ent, 0.0);
        let vs: Vec<f32> = batches[&TextureSlot::Fire(0)]
            .iter()
            .map(|v| v.uv[1])
            .collect();
        let (lo, hi) = (5.0 / FIRE_FRAMES, 6.0 / FIRE_FRAMES);
        assert!(vs
            .iter()
            .all(|&v| (v - lo).abs() < 1e-5 || (v - hi).abs() < 1e-5));
    }
}
