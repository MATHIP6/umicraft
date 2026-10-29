// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity meshing and rendering resources.
//!
//! - [`textures`]: the texture registry and its GPU upload.
//! - [`mobs`]: one builder per mob family, mapping a snapshot to mesh layers.
//! - [`player`]: the posed player model, shared with held items.
//! - [`hand`]: the first-person arm.
//!
//! Adding a new mob requires three steps:
//! 1. Add a [`TextureSlot`] variant in `umicraft-model`.
//! 2. Add an entry in `textures::texture_specs` with the jar path and fallback.
//! 3. Add the kind to the matching family builder in [`mobs`].

mod hand;
mod mobs;
mod player;
mod textures;

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;

pub use hand::hand_vertices;
pub(crate) use player::posed_player;
pub use textures::{load_or_fallback, procedural_dirt_texture, EntityTextures};

/// Meshes entity snapshots into batched vertex lists grouped by [`TextureSlot`].
#[must_use]
pub fn mesh_entities(
    snapshots: &[EntityRenderSnapshot],
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    let mut batches: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();

    for ent in snapshots {
        let world_pos = ent.position;
        let yaw_rad = ent.body_yaw.to_radians();
        // `RendererLivingEntity.rotateCorpse`: rotate(180 - yaw). The model faces -Z,
        // and a Minecraft yaw of zero looks towards +Z.
        let rot = Mat4::from_rotation_y(std::f32::consts::PI - yaw_rad);

        let sneaking = ent.kind == EntityKind::Player && ent.flags.is_sneaking();
        let pos = if sneaking {
            Vec3::new(world_pos.x, world_pos.y + 0.2, world_pos.z)
        } else {
            world_pos
        };

        for layer in mobs::build_entity_layers(ent) {
            let batch = batches.entry(layer.slot).or_default();
            for mv in layer.vertices {
                batch.push(transform_vertex(mv, rot, pos));
            }
        }
    }

    batches
}

fn transform_vertex(mv: ModelVertex, rot: Mat4, world_pos: Vec3) -> EntityVertex {
    let p = rot.transform_point3(Vec3::from(mv.position));
    let n = rot.transform_vector3(Vec3::from(mv.normal));
    EntityVertex {
        position: [p.x + world_pos.x, p.y + world_pos.y, p.z + world_pos.z],
        normal: n.to_array(),
        uv: mv.uv,
        light: 0x00FF_00FF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId};

    #[test]
    fn a_mob_with_yaw_zero_reaches_out_towards_south() {
        // Minecraft looks towards +Z at yaw 0, and a zombie holds its arms forward.
        let zombie = |yaw| EntityRenderSnapshot {
            id: EntityId(1),
            kind: EntityKind::Zombie,
            position: Vec3::ZERO,
            yaw,
            pitch: 0.0,
            head_yaw: yaw,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            uuid: None,
            held: None,
            item: None,
            age: 0.0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
        };
        let reach = |yaw: f32| {
            let batches = mesh_entities(&[zombie(yaw)]);
            let z = batches.values().flatten().map(|v| v.position[2]);
            let (min, max) = z.fold((f32::MAX, f32::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
            max + min
        };
        assert!(reach(0.0) > 0.1, "faces +Z at yaw 0");
        assert!(reach(180.0) < -0.1, "faces -Z at yaw 180");
    }

    #[test]
    fn empty_snapshots_produce_empty_batches() {
        let batches = mesh_entities(&[]);
        assert!(batches.is_empty());
    }

    #[test]
    fn live_entities_mesh_into_their_respective_batches() {
        let snapshots = [
            EntityRenderSnapshot {
                id: EntityId(1),
                kind: EntityKind::Player,
                position: Vec3::new(10.0, 64.0, 10.0),
                yaw: 0.0,
                body_yaw: 0.0,
                pitch: 0.0,
                head_yaw: 0.0,
                limb_swing: 0.5,
                limb_swing_amount: 0.3,
                uuid: Some([0; 16]),
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
            EntityRenderSnapshot {
                id: EntityId(2),
                kind: EntityKind::Zombie,
                position: Vec3::new(12.0, 64.0, 10.0),
                yaw: 90.0,
                body_yaw: 90.0,
                pitch: 0.0,
                head_yaw: 90.0,
                limb_swing: 0.0,
                limb_swing_amount: 0.0,
                uuid: None,
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
            EntityRenderSnapshot {
                id: EntityId(3),
                kind: EntityKind::Cow,
                position: Vec3::new(14.0, 64.0, 10.0),
                yaw: 180.0,
                body_yaw: 180.0,
                pitch: 0.0,
                head_yaw: 180.0,
                limb_swing: 1.0,
                limb_swing_amount: 0.5,
                uuid: None,
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
            // A dropped item has no entity mesh: P3 draws it from its item model.
            EntityRenderSnapshot {
                id: EntityId(4),
                kind: EntityKind::Item,
                position: Vec3::new(16.0, 64.25, 10.0),
                yaw: 45.0,
                body_yaw: 45.0,
                pitch: 0.0,
                head_yaw: 45.0,
                limb_swing: 0.0,
                limb_swing_amount: 0.0,
                uuid: None,
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
        ];

        let batches = mesh_entities(&snapshots);
        assert_eq!(batches[&TextureSlot::Steve].len(), 432);
        assert_eq!(batches[&TextureSlot::Zombie].len(), 432);
        assert_eq!(batches[&TextureSlot::Cow].len(), 324);
        assert_eq!(batches.len(), 3, "the dropped item adds no entity batch");
        assert!(!batches.contains_key(&TextureSlot::Alex));
    }
}
