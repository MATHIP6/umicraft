// SPDX-License-Identifier: MIT OR Apache-2.0

//! The leash of a mob (MCP `RenderLiving.renderLeash`).
//!
//! A rope of 24 segments from the side of the mob to its holder, sagging
//! between them, drawn as two crossed ribbons of 0.025 blocks whose segments
//! alternate between a light and a dark brown.

use std::collections::HashMap;

use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// Segments of the rope.
const SEGMENTS: usize = 24;
/// Thickness of the ribbons.
const THICKNESS: f64 = 0.025;
/// Where the two browns are on the 2 x 1 texture of the rope.
const DARK_UV: [f32; 2] = [0.25, 0.5];
const LIGHT_UV: [f32; 2] = [0.75, 0.5];

/// Adds the leash of `ent` to `batches` when it is leashed.
pub(super) fn add_leash(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
) {
    let Some(anchor) = ent.leash else {
        return;
    };
    let (yaw, pitch) = (
        f64::from(anchor.yaw).to_radians(),
        f64::from(anchor.pitch).to_radians(),
    );
    let (mut cos_yaw, mut sin_yaw, mut sin_pitch) = (yaw.cos(), yaw.sin(), pitch.sin());
    if anchor.hanging {
        (cos_yaw, sin_yaw, sin_pitch) = (0.0, 0.0, -1.0);
    }
    let cos_pitch = pitch.cos();
    let holder = anchor.position.as_dvec3();
    let (hx, hy, hz) = (
        holder.x - cos_yaw * 0.7 - sin_yaw * 0.5 * cos_pitch,
        holder.y + f64::from(anchor.eye_height) * 0.7 - sin_pitch * 0.5 - 0.25,
        holder.z - sin_yaw * 0.7 + cos_yaw * 0.5 * cos_pitch,
    );
    // The rope leaves the side of the mob, `width * 0.4` out from its middle.
    let width = f64::from(ent.kind.width());
    let side = f64::from(ent.body_yaw).to_radians() + std::f64::consts::FRAC_PI_2;
    let pos = ent.position.as_dvec3();
    let (ox, oz) = (side.cos() * width * 0.4, side.sin() * width * 0.4);
    let (dx, dy, dz) = (
        f64::from((hx - (pos.x + ox)) as f32),
        f64::from((hy - pos.y) as f32),
        f64::from((hz - (pos.z + oz)) as f32),
    );
    let (bx, by, bz) = (
        pos.x + ox,
        pos.y - (1.6 - f64::from(ent.kind.height())) * 0.5,
        pos.z + oz,
    );
    let out = batches.entry(TextureSlot::Leash).or_default();
    // Two ribbons, one after the other: the second is the first moved by
    // `(0, 0.025, 0)` at one edge and by `(0.025, 0, 0.025)` at the other.
    for ribbon in 0..2 {
        let mut strip = Vec::with_capacity(2 * (SEGMENTS + 1));
        for j in 0..=SEGMENTS {
            let f = j as f64 / SEGMENTS as f64;
            let at = [
                bx + dx * f,
                by + dy * (f * f + f) * 0.5 + (SEGMENTS - j) as f64 / 18.0 + 0.125,
                bz + dz * f,
            ];
            let uv = if j % 2 == 0 { DARK_UV } else { LIGHT_UV };
            let vertex = |offset: [f64; 3]| EntityVertex {
                position: [
                    (at[0] + offset[0]) as f32,
                    (at[1] + offset[1]) as f32,
                    (at[2] + offset[2]) as f32,
                ],
                normal: [0.0, 1.0, 0.0],
                uv,
                // Drawn without lighting: always fully bright.
                light: 0xFF,
            };
            if ribbon == 0 {
                strip.push(vertex([0.0, 0.0, 0.0]));
                strip.push(vertex([THICKNESS, THICKNESS, 0.0]));
            } else {
                strip.push(vertex([0.0, THICKNESS, 0.0]));
                strip.push(vertex([THICKNESS, 0.0, THICKNESS]));
            }
        }
        for t in 0..strip.len() - 2 {
            out.extend([strip[t], strip[t + 1], strip[t + 2]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use umicraft_game::entity::leash::LeashAnchor;
    use umicraft_game::entity::{EntityId, EntityKind};

    fn leashed(holder: LeashAnchor) -> Vec<EntityVertex> {
        let mut cow =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Cow, Vec3::new(0.0, 64.0, 0.0));
        cow.leash = Some(holder);
        let mut batches = HashMap::new();
        add_leash(&mut batches, &cow);
        batches.remove(&TextureSlot::Leash).unwrap_or_default()
    }

    fn player_at(x: f32, z: f32) -> LeashAnchor {
        LeashAnchor {
            position: Vec3::new(x, 64.0, z),
            yaw: 0.0,
            pitch: 0.0,
            eye_height: 1.62,
            hanging: false,
        }
    }

    /// The last vertex of the first ribbon.
    fn end_of_first_ribbon(rope: &[EntityVertex]) -> EntityVertex {
        // Each ribbon is `2 * SEGMENTS` triangles; the last triangle of the
        // first one ends on the far edge of the last segment.
        rope[SEGMENTS * 2 * 3 - 1]
    }

    #[test]
    fn a_rope_is_two_ribbons_of_twenty_four_segments() {
        assert_eq!(leashed(player_at(3.0, 4.0)).len(), 2 * SEGMENTS * 2 * 3);
    }

    #[test]
    fn the_rope_ends_on_the_holder() {
        let rope = leashed(player_at(3.0, 4.0));
        // Yaw 0: the hand is 0.7 along -x and 0.5 along +z of the player; the
        // far edge of the ribbon is 0.025 to the side.
        let last = end_of_first_ribbon(&rope);
        assert!(
            (last.position[0] - (3.0 - 0.7 + 0.025)).abs() < 1e-3,
            "{last:?}"
        );
        assert!((last.position[2] - (4.0 + 0.5)).abs() < 1e-3, "{last:?}");
    }

    #[test]
    fn the_segments_alternate_between_two_browns() {
        let rope = leashed(player_at(3.0, 4.0));
        let uvs: std::collections::BTreeSet<_> =
            rope.iter().map(|v| (v.uv[0] * 100.0) as i32).collect();
        assert_eq!(uvs, [25, 75].into_iter().collect());
    }

    #[test]
    fn a_rope_on_a_knot_comes_from_above() {
        let knot = LeashAnchor {
            position: Vec3::new(5.5, 64.5, 5.5),
            yaw: 0.0,
            pitch: 0.0,
            eye_height: 0.0,
            hanging: true,
        };
        let last = end_of_first_ribbon(&leashed(knot));
        // `d2 = d3 = 0` and `d4 = -1`: no shift sideways, the end is above.
        assert!((last.position[0] - (5.5 + 0.025)).abs() < 1e-3);
        assert!((last.position[2] - 5.5).abs() < 1e-3);
    }

    #[test]
    fn an_unleashed_mob_has_no_rope() {
        let cow = EntityRenderSnapshot::new(EntityId(1), EntityKind::Cow, Vec3::ZERO);
        let mut batches = HashMap::new();
        add_leash(&mut batches, &cow);
        assert!(batches.is_empty());
    }
}
