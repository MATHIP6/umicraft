// SPDX-License-Identifier: MIT OR Apache-2.0

//! The beam a guardian charges at its target (MCP `RenderGuardian.doRender`).
//!
//! From the guardian's eyes to the middle of its target, two crossed quads
//! 0.2 from the axis run along the beam and a square caps its far end, all
//! turning about the axis and with `entity/guardian_beam.png` scrolling
//! along. The beam is unlit, not blended and writes depth; its colour goes
//! from purple to yellow as it charges. It is drawn in the tinted pipeline
//! (`EntityRenderer::render_tinted`), the colour packed in the vertex light.
//!
//! Difference: the reference turns and scrolls the beam by the total world
//! time, which this client does not track; the guardian's own age stands in,
//! so only the phase differs.

use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::{DMat4, DVec3, Vec3};
use umicraft_game::combat::hitbox::size_of;
use umicraft_game::entity::mob_state::MobPose;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// How far the side quads are from the axis (`d2 = i * 0.2`).
const RADIUS: f64 = 0.2;
/// How far the corners of the end cap are (`d3 = d2 * 1.41`).
const CAP_RADIUS: f64 = RADIUS * 1.41;
/// The turn of the beam per tick (`f2 * 0.05 * (1 - 2.5)` with `i = 1`).
const TURN_PER_TICK: f64 = 0.05 * (1.0 - 2.5);
/// The right edge of the side quads in the texture (`0.4999`, not `0.5`, so
/// the left half of the strip never samples the cap's half).
const SIDE_U: f64 = 0.4999;

/// Adds the beam of the guardian `ent` to `batches` when it beams at
/// something.
pub(super) fn add_beam(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
) {
    if ent.kind != EntityKind::Guardian {
        return;
    }
    let (Some(target), MobPose::Guardian(pose)) = (ent.beam, ent.mob) else {
        return;
    };
    // `EntityGuardian.getEyeHeight`: half its height.
    let Some((_, height)) = size_of(ent.kind, &ent.meta) else {
        return;
    };
    let eyes = ent.position.as_dvec3() + DVec3::Y * (height * 0.5);
    let even_tick = (ent.age as u32).is_multiple_of(2);
    let vertices = beam_vertices(eyes, target.as_dvec3(), pose.charge, (ent.age, even_tick));
    batches
        .entry(TextureSlot::GuardianBeam)
        .or_default()
        .extend(vertices);
}

/// The colour of a beam `charge` (`0..=1`) into its charge, as the vertex
/// light word of the tinted pipeline (red, green, blue in the low bytes).
///
/// `j = 64 + (int)(f² * 240)` and the others go past 255 near the end of the
/// charge; `WorldRenderer.color` stores each one as a `(byte)`, so they wrap.
fn tint(charge: f32) -> u32 {
    let square = charge * charge;
    let red = 64 + (square * 240.0) as i32;
    let green = 32 + (square * 192.0) as i32;
    let blue = 128 - (square * 64.0) as i32;
    // The wrap of the `(byte)` cast is the behaviour copied.
    let byte = |channel: i32| u32::from(channel as u8);
    byte(red) | (byte(green) << 8) | (byte(blue) << 16)
}

/// The triangles of a beam from `from` to `to`, `charge` into its charge,
/// at `clock` ticks (`(world time + partial)` in the reference) on a tick that
/// is even or not (which half of the cap texture shows).
fn beam_vertices(
    from: DVec3,
    to: DVec3,
    charge: f32,
    (clock, even_tick): (f32, bool),
) -> Vec<EntityVertex> {
    let delta = to - from;
    // The beam overshoots the middle of the target by a block.
    let length = delta.length() + 1.0;
    let direction = delta.normalize_or_zero();
    if direction == DVec3::ZERO {
        return Vec::new();
    }
    // `rotate(90 - atan2(z, x), y)` then `rotate(acos(y), x)` turn +y along
    // the beam.
    let placed = DMat4::from_translation(from)
        * DMat4::from_rotation_y(FRAC_PI_2 - direction.z.atan2(direction.x))
        * DMat4::from_rotation_x(direction.y.clamp(-1.0, 1.0).acos());
    let clock = f64::from(clock);
    let turn = clock * TURN_PER_TICK;
    let around = |angle: f64, radius: f64| (angle.cos() * radius, angle.sin() * radius);
    // `d22 = -1 + f3` with `f3 = f2 * 0.5 % 1`, and `d23` a texel per
    // 0.4 blocks further.
    let near_v = -1.0 + (clock * 0.5).rem_euclid(1.0);
    let far_v = length * (0.5 / RADIUS) + near_v;
    let light = tint(charge);
    let vertex = |(x, z): (f64, f64), y: f64, (u, v): (f64, f64)| EntityVertex {
        position: placed
            .transform_point3(DVec3::new(x, y, z))
            .as_vec3()
            .to_array(),
        normal: Vec3::Y.to_array(),
        uv: [u as f32, v as f32],
        light,
    };
    // Each side quad spans two opposite angles: `PI` to `0`, then `PI / 2`
    // to `3 PI / 2`.
    let side = |first: f64, second: f64| {
        let (a, b) = (around(turn + first, RADIUS), around(turn + second, RADIUS));
        [
            vertex(a, length, (SIDE_U, far_v)),
            vertex(a, 0.0, (SIDE_U, near_v)),
            vertex(b, 0.0, (0.0, near_v)),
            vertex(b, length, (0.0, far_v)),
        ]
    };
    let cap_v = if even_tick { 0.5 } else { 0.0 };
    let cap = [
        vertex(
            around(turn + 3.0 * FRAC_PI_4, CAP_RADIUS),
            length,
            (0.5, cap_v + 0.5),
        ),
        vertex(
            around(turn + FRAC_PI_4, CAP_RADIUS),
            length,
            (1.0, cap_v + 0.5),
        ),
        vertex(
            around(turn + 7.0 * FRAC_PI_4, CAP_RADIUS),
            length,
            (1.0, cap_v),
        ),
        vertex(
            around(turn + 5.0 * FRAC_PI_4, CAP_RADIUS),
            length,
            (0.5, cap_v),
        ),
    ];
    [side(PI, 0.0), side(FRAC_PI_2, 3.0 * FRAC_PI_2), cap]
        .into_iter()
        .flat_map(|[a, b, c, d]| [a, b, c, a, c, d])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::meta::Variant;
    use umicraft_game::entity::mob_state::GuardianPose;
    use umicraft_game::entity::EntityId;

    fn guardian(beam: Option<Vec3>) -> EntityRenderSnapshot {
        let mut ent =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Guardian, Vec3::new(0.0, 60.0, 0.0));
        ent.meta.variant = Variant::Guardian {
            moving: false,
            elder: false,
            target: 2,
        };
        ent.mob = MobPose::Guardian(GuardianPose {
            charge: 0.5,
            ..GuardianPose::default()
        });
        ent.beam = beam;
        ent
    }

    fn beam(ent: &EntityRenderSnapshot) -> Vec<EntityVertex> {
        let mut batches = HashMap::new();
        add_beam(&mut batches, ent);
        batches
            .remove(&TextureSlot::GuardianBeam)
            .unwrap_or_default()
    }

    #[test]
    fn a_beam_runs_from_the_eyes_a_block_past_the_target() {
        let target = Vec3::new(6.0, 60.0 + 0.425, 8.0);
        let vertices = beam(&guardian(Some(target)));
        assert_eq!(
            vertices.len(),
            3 * 6,
            "two sides and a cap, two triangles each"
        );
        let eyes = Vec3::new(0.0, 60.0 + 0.85 * 0.5, 0.0);
        let axis = (target - eyes).normalize();
        let along: Vec<f32> = vertices
            .iter()
            .map(|vertex| (Vec3::from(vertex.position) - eyes).dot(axis))
            .collect();
        let near = along.iter().copied().fold(f32::MAX, f32::min);
        let far = along.iter().copied().fold(f32::MIN, f32::max);
        assert!(near.abs() < 1e-4, "starts at the eyes: {near}");
        assert!((far - 11.0).abs() < 1e-3, "10 to the target, 1 past: {far}");
        for vertex in &vertices {
            let offset = Vec3::from(vertex.position) - eyes;
            let off_axis = (offset - axis * offset.dot(axis)).length();
            assert!(off_axis < 0.29, "within the cap corners: {off_axis}");
        }
    }

    #[test]
    fn no_target_or_another_kind_draws_no_beam() {
        assert!(beam(&guardian(None)).is_empty());
        let mut squid = guardian(Some(Vec3::ONE));
        squid.kind = EntityKind::Squid;
        assert!(beam(&squid).is_empty());
    }

    #[test]
    fn the_beam_turns_from_purple_to_yellow_and_wraps_at_the_end() {
        let channels = |word: u32| [word & 0xFF, (word >> 8) & 0xFF, (word >> 16) & 0xFF];
        assert_eq!(channels(tint(0.0)), [64, 32, 128], "purple");
        // f² = 0.64: (64 + 153, 32 + 122, 128 - 40).
        assert_eq!(channels(tint(0.8)), [217, 154, 88]);
        // Fully charged the red is 304, stored as the byte 48.
        assert_eq!(channels(tint(1.0)), [48, 224, 64]);
    }

    #[test]
    fn the_texture_scrolls_and_the_cap_flips_with_the_ticks() {
        let from = DVec3::ZERO;
        let to = DVec3::new(0.0, 0.0, 4.0);
        let at = |clock: f32, even: bool| beam_vertices(from, to, 0.0, (clock, even));
        // Half a texture per tick (`f2 * 0.5 % 1`).
        let shift = at(1.0, true)[1].uv[1] - at(0.0, true)[1].uv[1];
        assert!((shift - 0.5).abs() < 1e-5, "{shift}");
        let cap_v = |vertices: &[EntityVertex]| {
            vertices[12..]
                .iter()
                .map(|v| v.uv[1])
                .fold(f32::MAX, f32::min)
        };
        assert_eq!(cap_v(&at(0.0, true)), 0.5);
        assert_eq!(cap_v(&at(0.0, false)), 0.0);
    }
}
