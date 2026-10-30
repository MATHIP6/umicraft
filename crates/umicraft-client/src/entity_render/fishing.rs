// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bobber of a fishing rod and its line (MCP `RenderFish`).
//!
//! The bobber is a 0.5 block square of the particle sheet turned to the
//! camera. The line is a black curve of 16 segments from the tip of the rod of
//! the angler to the bobber; the reference draws it as a one pixel GL line,
//! here it is a thin ribbon that turns to the camera.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// Width of the ribbon of the line, in blocks.
const LINE_WIDTH: f32 = 0.012;
/// Segments of the line.
const SEGMENTS: usize = 16;

/// The camera of the frame: where it is and where it looks (radians, in the
/// convention of `umicraft_render::Camera`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Eye {
    pub(super) position: Vec3,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
}

/// The tip of the rod of `angler` seen from outside (`RenderFish.doRender`,
/// the branch of the other players and of the third person view), and the
/// extra drop of a sneaking angler.
fn rod_tip(angler: &EntityRenderSnapshot) -> (Vec3, f32) {
    let yaw = angler.body_yaw.to_radians();
    let (sin, cos) = yaw.sin_cos();
    let sneaking = angler.flags.is_sneaking();
    let eye = if sneaking { 1.54 } else { 1.62 };
    let tip = Vec3::new(
        angler.position.x - cos * 0.35 - sin * 0.8,
        angler.position.y + eye - 0.45,
        angler.position.z - sin * 0.35 + cos * 0.8,
    );
    (tip, if sneaking { -0.1875 } else { 0.0 })
}

/// The points of the line from the bobber at `hook` to the tip of the rod.
fn line_points(hook: Vec3, tip: Vec3, drop: f32) -> Vec<Vec3> {
    let dx = tip.x - hook.x;
    let dy = tip.y - (hook.y + 0.25) + drop;
    let dz = tip.z - hook.z;
    (0..=SEGMENTS)
        .map(|at| {
            let f = at as f32 / SEGMENTS as f32;
            Vec3::new(
                hook.x + dx * f,
                hook.y + dy * (f * f + f) * 0.5 + 0.25,
                hook.z + dz * f,
            )
        })
        .collect()
}

/// Adds the bobber `ent` and its line to `batches` when it is one. `snapshots`
/// is the frame, where the angler is looked up by the id in `object_data`; a
/// bobber whose angler is not in it (the local player) gets no line.
pub(super) fn add_bobber(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    snapshots: &[EntityRenderSnapshot],
    light: u32,
    eye: Eye,
) {
    if ent.kind != EntityKind::FishHook {
        return;
    }
    let placed = Mat4::from_translation(ent.position)
        * Mat4::from_rotation_y((PI - (eye.yaw - PI)).rem_euclid(TAU))
        * Mat4::from_rotation_x(eye.pitch)
        * Mat4::from_scale(Vec3::splat(0.5));
    let normal = placed
        .transform_vector3(Vec3::Y)
        .normalize_or_zero()
        .to_array();
    let corner = |x: f32, y: f32, u: f32, v: f32| EntityVertex {
        position: placed.transform_point3(Vec3::new(x, y, 0.0)).to_array(),
        normal,
        uv: [u, v],
        light,
    };
    let quad = [
        corner(-0.5, -0.5, 0.0625, 0.1875),
        corner(0.5, -0.5, 0.125, 0.1875),
        corner(0.5, 0.5, 0.125, 0.125),
        corner(-0.5, 0.5, 0.0625, 0.125),
    ];
    batches
        .entry(TextureSlot::Particles)
        .or_default()
        .extend([quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);

    let Some(angler) = snapshots.iter().find(|other| other.id.0 == ent.object_data) else {
        return;
    };
    let (tip, drop) = rod_tip(angler);
    let points = line_points(ent.position, tip, drop);
    let ribbon = batches.entry(TextureSlot::FishLine).or_default();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let to_eye = eye.position - (a + b) * 0.5;
        let side = (b - a).cross(to_eye).normalize_or_zero() * (LINE_WIDTH / 2.0);
        let vertex = |position: Vec3| EntityVertex {
            position: position.to_array(),
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            light,
        };
        let [a0, a1, b0, b1] = [a - side, a + side, b - side, b + side].map(vertex);
        ribbon.extend([a0, a1, b1, a0, b1, b0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn eye() -> Eye {
        Eye {
            position: Vec3::new(0.0, 66.0, -10.0),
            yaw: PI,
            pitch: 0.0,
        }
    }

    fn angler() -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(5), EntityKind::Player, Vec3::new(0.0, 64.0, 0.0))
    }

    fn scene(with_angler: bool) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let mut hook =
            EntityRenderSnapshot::new(EntityId(6), EntityKind::FishHook, Vec3::new(3.0, 64.0, 6.0));
        hook.object_data = 5;
        let snapshots = if with_angler {
            vec![angler(), hook.clone()]
        } else {
            vec![hook.clone()]
        };
        let mut batches = HashMap::new();
        add_bobber(&mut batches, &hook, &snapshots, 0xFF, eye());
        batches
    }

    #[test]
    fn the_bobber_is_a_half_block_square_of_the_particle_sheet() {
        let batches = scene(true);
        let sprite = &batches[&TextureSlot::Particles];
        assert_eq!(sprite.len(), 6);
        let xs: Vec<f32> = sprite.iter().map(|v| v.position[0]).collect();
        let width = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!((width - 0.5).abs() < 1e-4);
    }

    #[test]
    fn the_line_runs_from_the_bobber_to_the_tip_of_the_rod() {
        assert_eq!(scene(true)[&TextureSlot::FishLine].len(), SEGMENTS * 6);
        // The tip of the rod of a player facing south: 0.35 west, 0.8 ahead,
        // at the height of the eyes less 0.45.
        let (tip, _) = rod_tip(&angler());
        assert!(
            (tip - Vec3::new(-0.35, 65.17, 0.8)).length() < 1e-4,
            "{tip}"
        );
        let points = line_points(Vec3::new(3.0, 64.0, 6.0), tip, 0.0);
        assert_eq!(points.len(), SEGMENTS + 1);
        assert!((points[0] - Vec3::new(3.0, 64.25, 6.0)).length() < 1e-4);
        assert!((points[SEGMENTS] - tip).length() < 1e-4);
        // With the rod above the bobber the line sags under the straight one.
        let straight = (points[0] + points[SEGMENTS]) / 2.0;
        assert!(points[SEGMENTS / 2].y < straight.y);
    }

    #[test]
    fn a_bobber_without_its_angler_has_no_line() {
        let batches = scene(false);
        assert!(batches.contains_key(&TextureSlot::Particles));
        assert!(!batches.contains_key(&TextureSlot::FishLine));
    }

    #[test]
    fn a_sneaking_angler_holds_the_rod_lower() {
        let mut angler = angler();
        let standing = rod_tip(&angler);
        angler.flags = umicraft_game::entity::EntityFlags::new(0x02, 0);
        let sneaking = rod_tip(&angler);
        assert!(sneaking.0.y < standing.0.y);
        assert_eq!(sneaking.1, -0.1875);
    }
}
