// SPDX-License-Identifier: MIT OR Apache-2.0

//! The light of a dying ender dragon (MCP `LayerEnderDragonDeath`).
//!
//! From the moment it dies and for 200 ticks the dragon throws rays: fans of
//! triangles from a point in its body, white at the root and magenta fading to
//! nothing at the tip, added to the frame. There are more of them as the death
//! goes on, and in the last fifth they grow and the white at the root fades.
//!
//! The reference colours each vertex; here the colour comes from a 64 x 1
//! texture that runs from the root (`u = 0`) to the tip, with the alpha already
//! multiplied in, and the fading of the root is the tint of the layer.

use glam::{Mat4, Vec3};
use umicraft_model::entity::ModelVertex;

use crate::java_random::JavaRandom;

/// Ticks of the death of a dragon.
const DEATH_TICKS: f32 = 200.0;
/// The height of the model origin over the feet, in blocks (the 24 pixels the
/// models of this crate put their pivots under).
const MODEL_HEIGHT: f32 = 1.5;
/// Where the tip of a ray samples the texture: just short of the edge, which
/// repeats.
const TIP_U: f32 = 0.98;

/// The texels of the ray texture: white to magenta from the root to the tip,
/// multiplied by an alpha that falls from 1 to 0.
#[must_use]
pub(super) fn ray_texture() -> (Vec<u8>, u32, u32) {
    let mut rgba = Vec::with_capacity(64 * 4);
    for texel in 0..64 {
        let along = texel as f32 / 63.0;
        let alpha = 1.0 - along;
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        rgba.extend([byte(alpha), byte((1.0 - along) * alpha), byte(alpha), 255]);
    }
    (rgba, 64, 1)
}

/// The rays of a dragon `death_ticks` ticks (and the fraction) after its death,
/// in the space of this crate's models, and the brightness of their root.
#[must_use]
pub(super) fn death_rays(death_ticks: f32) -> (Vec<ModelVertex>, f32) {
    let progress = death_ticks / DEATH_TICKS;
    let finale = if progress > 0.8 {
        (progress - 0.8) / 0.2
    } else {
        0.0
    };
    let count = ((progress + progress * progress) / 2.0 * 60.0).ceil() as usize;
    let mut random = JavaRandom::new(432);
    // `translate(0, -1, -2)` in the flipped frame of the reference; every
    // rotation goes on top of the one before.
    let mut frame = Mat4::from_translation(Vec3::new(0.0, -1.0, -2.0));
    let mut vertices = Vec::with_capacity(count * 9);
    for _ in 0..count {
        for (axis, extra) in [
            (Vec3::X, 0.0),
            (Vec3::Y, 0.0),
            (Vec3::Z, 0.0),
            (Vec3::X, 0.0),
            (Vec3::Y, 0.0),
            (Vec3::Z, progress * 90.0),
        ] {
            let degrees = random.next_float() * 360.0 + extra;
            frame *= Mat4::from_axis_angle(axis, degrees.to_radians());
        }
        let length = random.next_float() * 20.0 + 5.0 + finale * 10.0;
        let width = random.next_float() * 2.0 + 1.0 + finale * 2.0;
        let corner = |x: f32, y: f32, z: f32, u: f32| {
            let at = frame.transform_point3(Vec3::new(x, y, z));
            // The reference frame is flipped about z: this crate's models have
            // the height going up from the feet and x mirrored by the entity.
            ModelVertex {
                position: [at.x, MODEL_HEIGHT - at.y, at.z],
                normal: [0.0, 1.0, 0.0],
                uv: [u, 0.5],
            }
        };
        let root = corner(0.0, 0.0, 0.0, 0.0);
        let left = corner(-0.866 * width, length, -0.5 * width, TIP_U);
        let right = corner(0.866 * width, length, -0.5 * width, TIP_U);
        let front = corner(0.0, length, width, TIP_U);
        vertices.extend([root, left, right, root, right, front, root, front, left]);
    }
    (vertices, 1.0 - finale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragon_that_just_died_has_few_rays_and_a_dying_one_many() {
        // `(f + f^2) / 2 * 60` rays, three triangles each.
        assert_eq!(death_rays(1.0).0.len(), 9);
        assert_eq!(death_rays(100.0).0.len(), 23 * 9);
        assert_eq!(death_rays(200.0).0.len(), 60 * 9);
    }

    #[test]
    fn the_root_fades_in_the_last_fifth() {
        assert_eq!(death_rays(100.0).1, 1.0);
        assert_eq!(death_rays(160.0).1, 1.0);
        assert!((death_rays(180.0).1 - 0.5).abs() < 1e-6);
        assert!(death_rays(200.0).1.abs() < 1e-6);
    }

    #[test]
    fn the_rays_are_the_same_every_time() {
        assert_eq!(death_rays(50.0).0, death_rays(50.0).0);
        assert_ne!(death_rays(50.0).0[..9], death_rays(150.0).0[..9]);
    }

    #[test]
    fn a_dying_dragon_dissolves_and_a_living_one_does_not() {
        use glam::Vec3;
        use umicraft_game::entity::flight::DragonFlight;
        use umicraft_game::entity::{EntityId, EntityKind, EntityRenderSnapshot};
        use umicraft_model::entity::TextureSlot;

        let layers = |death_ticks: f32| {
            let mut ent = EntityRenderSnapshot::new(EntityId(1), EntityKind::Dragon, Vec3::ZERO);
            let mut pose = DragonFlight::default().pose(1.0, true);
            pose.death_ticks = death_ticks;
            ent.flight = Some(pose);
            crate::entity_render::mobs::build_entity_layers_for(
                &ent,
                &crate::entity_render::skins::NoSkins,
                false,
            )
        };
        let living = layers(0.0);
        assert_eq!(living[0].slot, TextureSlot::Dragon);
        assert_eq!(living[0].cutoff, 0);
        // Half way through the 200 ticks the alpha test is at one half.
        let dying = layers(100.0);
        assert_eq!(dying[0].slot, TextureSlot::DragonExploding);
        assert_eq!(dying[0].cutoff, 16);
        assert_eq!(layers(200.0)[0].cutoff, 31);
        // The rays come with it.
        assert!(dying
            .iter()
            .any(|layer| layer.slot == TextureSlot::DragonRay));
    }

    #[test]
    fn the_texture_runs_from_white_to_nothing() {
        let (rgba, width, height) = ray_texture();
        assert_eq!((width, height, rgba.len()), (64, 1, 64 * 4));
        assert_eq!(&rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(&rgba[63 * 4..63 * 4 + 3], &[0, 0, 0]);
    }
}
