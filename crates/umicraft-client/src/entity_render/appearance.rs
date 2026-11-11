// SPDX-License-Identifier: MIT OR Apache-2.0

//! The elder guardian that haunts the screen when a server curses the player
//! (game state change 10, MCP `MobAppearance.renderParticle`).
//!
//! The particle is only a clock; this draws the elder guardian model from it:
//! it swings from above the player's head down to their feet over its 30
//! ticks, at 5 % opacity at both ends and 55 % in the middle.

use std::collections::HashMap;
use std::f32::consts::PI;

use glam::{Mat4, Vec3};
use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityId, EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

use super::skins::SkinLookup;
use super::{mobs, transform, transform_vertex, FULL_LIGHT};

/// Height of the point it swings about, above the camera (`translate(0, 1.8,
/// 0)`).
const PIVOT_HEIGHT: f32 = 1.8;
/// Where the model sits from that point (`translate(0, -0.4, -1.5)`).
const MODEL_OFFSET: Vec3 = Vec3::new(0.0, -0.4, -1.5);
/// The scale it is drawn at (`MobAppearance.java:57`).
const SCALE: f32 = 0.425_531_92;
/// The pitch it swings from, and the sweep over its life, in degrees.
const SWING_START: f32 = 60.0;
const SWING_SWEEP: f32 = 150.0;
/// Shift of the opacity bits in [`EntityVertex::light`] (see
/// `entity.wgsl`).
const OPACITY_SHIFT: u32 = 19;

/// The opacity of the apparition `progress` (`0..=1`) into its life
/// (`0.05 + 0.5 * sin(progress * PI)`, MCP `MobAppearance.java:54`), in
/// 255ths for the vertex bits, never 0 (which means the spectator's 15 %).
fn opacity_bits(progress: f32) -> u32 {
    let opacity = 0.05 + 0.5 * (progress * PI).sin();
    ((opacity * 255.0).round() as u32).clamp(1, 255)
}

/// The matrix that puts the model in the world: it hangs on the camera, turns
/// with its look and swings down as `progress` runs (MCP
/// `MobAppearance.renderParticle`). `yaw` and `pitch` are the camera's, in
/// radians and its convention (zero yaw looks north).
fn placement(camera: Vec3, yaw: f32, pitch: f32, progress: f32) -> Mat4 {
    // The reference's own yaw is clockwise from south.
    let look_yaw = (yaw - PI).to_degrees();
    let look_pitch = pitch.to_degrees();
    Mat4::from_translation(camera)
        * Mat4::from_translation(Vec3::new(0.0, PIVOT_HEIGHT, 0.0))
        * Mat4::from_rotation_y((180.0 - look_yaw).to_radians())
        * Mat4::from_rotation_x((SWING_START - SWING_SWEEP * progress - look_pitch).to_radians())
        * Mat4::from_translation(MODEL_OFFSET)
        * Mat4::from_scale(Vec3::splat(SCALE))
}

/// The vertices of the elder guardian at `progress` of its apparition, by
/// texture, to be drawn as a ghost layer. `camera` is the eye in the space of
/// the entity vertices.
#[must_use]
pub(super) fn elder_guardian(
    skins: &dyn SkinLookup,
    camera: Vec3,
    (yaw, pitch): (f32, f32),
    progress: f32,
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    let mut guardian = EntityRenderSnapshot::new(EntityId(0), EntityKind::Guardian, Vec3::ZERO);
    guardian.meta.variant = Variant::Guardian {
        moving: false,
        elder: true,
        target: 0,
    };
    // The model of any other entity: facing south at the origin.
    let matrix = placement(camera, yaw, pitch, progress) * transform::model_matrix(&guardian);
    let light = FULL_LIGHT | (opacity_bits(progress) << OPACITY_SHIFT);
    let mut meshes: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();
    for layer in mobs::build_entity_layers_for(&guardian, skins, false) {
        meshes.entry(layer.slot).or_default().extend(
            layer
                .vertices
                .into_iter()
                .map(|vertex| transform_vertex(vertex, &matrix, light)),
        );
    }
    meshes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_render::NoSkins;

    /// The camera looks south (the reference's yaw 0) and level.
    const SOUTH: (f32, f32) = (PI, 0.0);

    #[test]
    fn it_fades_in_and_out_around_a_translucent_peak() {
        assert_eq!(opacity_bits(0.0), 13, "5 %");
        assert_eq!(opacity_bits(1.0), 13);
        assert_eq!(opacity_bits(0.5), 140, "55 %");
    }

    #[test]
    fn it_draws_an_elder_guardian_ahead_of_the_camera() {
        let camera = Vec3::new(10.0, 65.62, -4.0);
        let meshes = elder_guardian(&NoSkins, camera, SOUTH, 0.4);
        let vertices = meshes
            .get(&TextureSlot::GuardianElder)
            .expect("the elder texture");
        assert!(!vertices.is_empty());
        // At 0.4 the swing is level: the guardian is a block and a half
        // ahead (south, +z) and above the eyes.
        let centre = vertices
            .iter()
            .map(|vertex| Vec3::from(vertex.position))
            .sum::<Vec3>()
            / vertices.len() as f32;
        let offset = centre - camera;
        assert!((offset.z - 1.5).abs() < 0.5, "{offset}");
        assert!(offset.x.abs() < 0.5 && offset.y > 1.0, "{offset}");
    }

    #[test]
    fn it_follows_the_look_of_the_camera() {
        let camera = Vec3::ZERO;
        let centre = |yaw: f32| {
            let meshes = elder_guardian(&NoSkins, camera, (yaw, 0.0), 0.4);
            let vertices = &meshes[&TextureSlot::GuardianElder];
            vertices
                .iter()
                .map(|vertex| Vec3::from(vertex.position))
                .sum::<Vec3>()
                / vertices.len() as f32
        };
        // Zero yaw looks north in the camera's convention: -z.
        assert!(centre(0.0).z < -1.0, "{}", centre(0.0));
        assert!(centre(PI).z > 1.0, "{}", centre(PI));
    }

    #[test]
    fn its_vertices_are_lit_fully_and_carry_their_opacity() {
        let meshes = elder_guardian(&NoSkins, Vec3::ZERO, SOUTH, 0.5);
        let vertices = &meshes[&TextureSlot::GuardianElder];
        for vertex in vertices {
            assert_eq!(vertex.light & 0xFF, FULL_LIGHT);
            assert_eq!((vertex.light >> OPACITY_SHIFT) & 0xFF, 140);
        }
    }
}
