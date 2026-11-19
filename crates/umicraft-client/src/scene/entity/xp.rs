// SPDX-License-Identifier: MIT OR Apache-2.0

//! Experience orbs (MCP `RenderXPOrb`).
//!
//! An orb is a square of the texture sheet that faces the camera, 0.3 blocks
//! wide, whose colour cycles from yellow to green with its age. The sheet holds
//! eleven sizes of orb, chosen by the experience the orb is worth.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// Bit of [`EntityVertex::light`] that colours the vertex as an orb, and the
/// shift of the 8-bit phase of that colour (read by `entity.wgsl`).
const ORB_BIT: u32 = 1 << 18;
const PHASE_SHIFT: u32 = 19;

/// `EntityXPOrb.getTextureByXP`: the cell of the sheet of an orb worth `xp`.
fn texture_cell(xp: i32) -> i32 {
    [2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3]
        .iter()
        .position(|&limit| xp >= limit)
        .map_or(0, |at| 10 - at as i32)
}

/// The phase of the colour of an orb of `age` ticks (`(xpColor + partial) / 2`
/// in the reference), in 256 steps of a turn.
fn phase_step(age: f32) -> u32 {
    ((age / 2.0).rem_euclid(TAU) / TAU * 256.0) as u32 & 0xFF
}

/// Adds the experience orb `ent` to `batches` when it is one. `view_yaw` and
/// `view_pitch` are those of the camera, in radians, in the convention of
/// `umicraft_render::Camera`.
pub(super) fn add_xp_orb(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    light: u32,
    view_yaw: f32,
    view_pitch: f32,
) {
    if ent.kind != EntityKind::XpOrb {
        return;
    }
    // `rotate(180 - playerViewY, y)`, `rotate(-playerViewX, x)`, `scale(0.3)`.
    let placed = Mat4::from_translation(ent.position)
        * Mat4::from_rotation_y((PI - (view_yaw - PI)).rem_euclid(TAU))
        * Mat4::from_rotation_x(view_pitch)
        * Mat4::from_scale(Vec3::splat(0.3));
    let cell = texture_cell(ent.object_data);
    let (u0, u1) = ((cell % 4) as f32 * 0.25, (cell % 4) as f32 * 0.25 + 0.25);
    let (v0, v1) = ((cell / 4) as f32 * 0.25, (cell / 4) as f32 * 0.25 + 0.25);
    let light = light | ORB_BIT | (phase_step(ent.age) << PHASE_SHIFT);
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
        corner(-0.5, -0.25, u0, v1),
        corner(0.5, -0.25, u1, v1),
        corner(0.5, 0.75, u1, v0),
        corner(-0.5, 0.75, u0, v0),
    ];
    batches
        .entry(TextureSlot::ExperienceOrb)
        .or_default()
        .extend([quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn orb(xp: i32, age: f32) -> EntityRenderSnapshot {
        let mut ent =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::XpOrb, Vec3::new(5.0, 64.0, 5.0));
        ent.object_data = xp;
        ent.age = age;
        ent
    }

    fn quad(ent: &EntityRenderSnapshot, yaw: f32, pitch: f32) -> Vec<EntityVertex> {
        let mut batches = HashMap::new();
        add_xp_orb(&mut batches, ent, 0xFF, yaw, pitch);
        batches
            .remove(&TextureSlot::ExperienceOrb)
            .unwrap_or_default()
    }

    fn range(vertices: &[EntityVertex], pick: impl Fn(&EntityVertex) -> f32) -> (f32, f32) {
        let values: Vec<f32> = vertices.iter().map(pick).collect();
        (
            values.iter().cloned().fold(f32::MAX, f32::min),
            values.iter().cloned().fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn the_size_of_an_orb_follows_the_experience_it_is_worth() {
        let cells: Vec<i32> = [1, 2, 3, 6, 7, 16, 17, 36, 37, 72, 73, 148, 149, 306, 307]
            .into_iter()
            .map(texture_cell)
            .collect();
        assert_eq!(cells, [0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7]);
        assert_eq!(texture_cell(616), 7);
        assert_eq!(texture_cell(617), 8);
        assert_eq!(texture_cell(1237), 9);
        assert_eq!(texture_cell(2477), 10);
        assert_eq!(texture_cell(32_767), 10);
    }

    #[test]
    fn the_cell_is_a_sixteen_pixel_square_of_the_sheet() {
        // Cell 6: column 2, row 1 of a 64 x 64 sheet of 16-pixel cells.
        let vertices = quad(&orb(149, 0.0), 0.0, 0.0);
        assert_eq!(range(&vertices, |v| v.uv[0]), (0.5, 0.75));
        assert_eq!(range(&vertices, |v| v.uv[1]), (0.25, 0.5));
    }

    #[test]
    fn the_orb_is_a_third_of_a_block_wide_and_faces_the_camera() {
        // The camera at yaw 0 looks towards -z: the square lies in the plane
        // of constant z, 0.3 wide, from 0.075 under the position to 0.225 over.
        let vertices = quad(&orb(1, 0.0), 0.0, 0.0);
        assert_eq!(vertices.len(), 6);
        assert!(vertices.iter().all(|v| (v.position[2] - 5.0).abs() < 1e-4));
        let (x0, x1) = range(&vertices, |v| v.position[0]);
        assert!((x1 - x0 - 0.3).abs() < 1e-4, "{}", x1 - x0);
        let (y0, y1) = range(&vertices, |v| v.position[1]);
        assert!((y0 - (64.0 - 0.075)).abs() < 1e-4, "{y0}");
        assert!((y1 - (64.0 + 0.225)).abs() < 1e-4, "{y1}");
        // A quarter turn of the camera turns the square with it.
        let turned = quad(&orb(1, 0.0), PI / 2.0, 0.0);
        assert!(turned.iter().all(|v| (v.position[0] - 5.0).abs() < 1e-4));
    }

    #[test]
    fn the_colour_cycles_with_the_age() {
        let word = |age: f32| quad(&orb(1, age), 0.0, 0.0)[0].light;
        assert_ne!(word(0.0) & ORB_BIT, 0);
        assert_eq!(word(0.0) & 0xFF, 0xFF, "the world light is kept");
        assert_eq!(word(0.0) >> PHASE_SHIFT, 0);
        // The phase is half the age: a turn of it takes 4 pi ticks.
        assert_eq!(phase_step(TAU), 128);
        assert_eq!(phase_step(4.0 * TAU), 0);
        assert_ne!(word(0.0), word(10.0));
    }

    #[test]
    fn only_orbs_are_drawn_as_orbs() {
        let mut ent = orb(1, 0.0);
        ent.kind = EntityKind::Pig;
        assert!(quad(&ent, 0.0, 0.0).is_empty());
    }
}
