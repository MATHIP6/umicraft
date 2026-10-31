// SPDX-License-Identifier: MIT OR Apache-2.0

//! The ray that recharges the dragon from an ender crystal
//! (MCP `RenderDragon.drawRechargeRay`).
//!
//! An alive dragon with a crystal within 32 blocks is tied to the nearest one by
//! a tube of eight faces, wide at the crystal and narrow at the dragon, whose
//! texture runs along it. The reference fades it from black at the dragon to
//! white at the crystal with the colour of its vertices; here the light of the
//! vertices does it.

use std::collections::HashMap;
use std::f32::consts::TAU;

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// How far from the dragon a crystal can heal it (`EntityDragon.updateDragonEnderCrystal`).
const RANGE: f32 = 32.0;
/// Faces of the tube.
const FACES: usize = 8;

/// The crystal a dragon is healed by: the nearest one in range.
fn nearest_crystal<'a>(
    dragon: &EntityRenderSnapshot,
    snapshots: &'a [EntityRenderSnapshot],
) -> Option<&'a EntityRenderSnapshot> {
    snapshots
        .iter()
        .filter(|other| other.kind == EntityKind::EnderCrystal)
        .map(|crystal| (crystal.position.distance(dragon.position), crystal))
        .filter(|(distance, _)| *distance <= RANGE)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, crystal)| crystal)
}

/// Adds the recharge ray of the dragon `ent` to `batches` when it has one.
pub(super) fn add_beam(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    snapshots: &[EntityRenderSnapshot],
) {
    let alive = ent.flight.is_some_and(|flight| flight.death_ticks <= 0.0);
    if ent.kind != EntityKind::Dragon || !alive {
        return;
    }
    let Some(crystal) = nearest_crystal(ent, snapshots) else {
        return;
    };
    // The crystal bobs by the same wave as its model (`add_crystal`).
    let spin = crystal.age + crystal.id.0.rem_euclid(100_000) as f32;
    let wave = (spin * 0.2).sin() / 2.0 + 0.5;
    let bob = (wave * wave + wave) * 0.2;
    let delta = Vec3::new(
        crystal.position.x - ent.position.x,
        bob + crystal.position.y - 1.0 - ent.position.y,
        crystal.position.z - ent.position.z,
    );
    let flat = delta.x.hypot(delta.z);
    let length = delta.length();
    // `rotate(-atan2(dz, dx) - 90, y)` then `rotate(-atan2(flat, dy) - 90, x)`
    // turn +z to point along `delta`.
    let placed = Mat4::from_translation(ent.position + Vec3::Y * 2.0)
        * Mat4::from_rotation_y(-delta.z.atan2(delta.x) - std::f32::consts::FRAC_PI_2)
        * Mat4::from_rotation_x(-flat.atan2(delta.y) - std::f32::consts::FRAC_PI_2);
    let scroll = ent.age * 0.01;
    let (far_v, near_v) = (length / 32.0 - scroll, -scroll);

    let mut strip = Vec::with_capacity(2 * (FACES + 1));
    for face in 0..=FACES {
        let angle = (face % FACES) as f32 * TAU / FACES as f32;
        let (x, y) = (angle.sin() * 0.75, angle.cos() * 0.75);
        let u = (face % FACES) as f32 / FACES as f32;
        let vertex = |position: Vec3, v: f32, light: u32| EntityVertex {
            position: placed.transform_point3(position).to_array(),
            normal: [0.0, 1.0, 0.0],
            uv: [u, v],
            light,
        };
        // Black at the dragon, white at the crystal.
        strip.push(vertex(Vec3::new(x * 0.2, y * 0.2, 0.0), far_v, 0x00));
        strip.push(vertex(Vec3::new(x, y, length), near_v, 0xFF));
    }
    let out = batches.entry(TextureSlot::DragonBeam).or_default();
    for t in 0..strip.len() - 2 {
        out.extend([strip[t], strip[t + 1], strip[t + 2]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::flight::DragonFlight;
    use umicraft_game::entity::EntityId;

    fn dragon(dead: bool) -> EntityRenderSnapshot {
        let mut ent =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Dragon, Vec3::new(0.0, 80.0, 0.0));
        let mut pose = DragonFlight::default().pose(1.0, true);
        pose.death_ticks = if dead { 10.0 } else { 0.0 };
        ent.flight = Some(pose);
        ent
    }

    fn crystal(id: i32, at: Vec3) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(id), EntityKind::EnderCrystal, at)
    }

    fn beam(dragon: &EntityRenderSnapshot, others: &[EntityRenderSnapshot]) -> Vec<EntityVertex> {
        let mut batches = HashMap::new();
        add_beam(&mut batches, dragon, others);
        batches.remove(&TextureSlot::DragonBeam).unwrap_or_default()
    }

    #[test]
    fn the_nearest_crystal_in_range_is_the_one_that_heals() {
        let ent = dragon(false);
        let near = crystal(2, Vec3::new(10.0, 70.0, 0.0));
        let far = crystal(3, Vec3::new(30.0, 70.0, 0.0));
        let out_of_range = crystal(4, Vec3::new(80.0, 70.0, 0.0));
        let all = [far, near.clone(), out_of_range.clone()];
        assert_eq!(nearest_crystal(&ent, &all).unwrap().id, near.id);
        assert!(nearest_crystal(&ent, &[out_of_range]).is_none());
    }

    #[test]
    fn the_ray_is_a_tube_of_eight_faces_from_the_dragon_to_the_crystal() {
        let ent = dragon(false);
        let target = crystal(2, Vec3::new(10.0, 70.0, 5.0));
        let ray = beam(&ent, std::slice::from_ref(&target));
        assert_eq!(ray.len(), 2 * FACES * 3);
        // The narrow black end sits on the dragon, 2 blocks up; the wide white
        // end at the crystal.
        let narrow: Vec<_> = ray.iter().filter(|v| v.light == 0x00).collect();
        let wide: Vec<_> = ray.iter().filter(|v| v.light == 0xFF).collect();
        assert!(!narrow.is_empty() && !wide.is_empty());
        for vertex in narrow {
            let from = Vec3::from(vertex.position) - Vec3::new(0.0, 82.0, 0.0);
            assert!(from.length() < 0.2, "{from}");
        }
        let centre = wide
            .iter()
            .fold(Vec3::ZERO, |sum, v| sum + Vec3::from(v.position))
            / wide.len() as f32;
        // The ray starts 2 blocks over the dragon but is aimed from its feet at
        // 1 block under the crystal, so it ends 1 block over the crystal (and
        // its bob).
        let aim = target.position + Vec3::Y;
        assert!(centre.distance(aim) < 0.6, "{centre} {aim}");
    }

    #[test]
    fn a_dragon_without_a_crystal_or_a_dead_one_has_no_ray() {
        let target = crystal(2, Vec3::new(10.0, 70.0, 5.0));
        assert!(beam(&dragon(false), &[]).is_empty());
        assert!(beam(&dragon(true), &[target]).is_empty());
    }
}
