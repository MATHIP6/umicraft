// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity meshing and rendering resources.
//!
//! - [`textures`]: the texture registry and its GPU upload.
//! - [`mobs`]: one builder per mob family, mapping a snapshot to mesh layers.
//! - [`player`]: the posed player model, shared with held items.
//! - [`pose`]: the humanoid pose of a snapshot.
//! - [`transform`]: where a model stands, turns, falls and sleeps.
//! - [`hand`]: the first-person arm.
//!
//! Adding a new mob requires three steps:
//! 1. Add a [`TextureSlot`] variant in `umicraft-model`.
//! 2. Add an entry in `textures::texture_specs` with the jar path and fallback.
//! 3. Add the kind to the matching family builder in [`mobs`].

mod hand;
mod mobs;
mod player;
mod pose;
mod textures;
mod transform;

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;

pub use hand::hand_vertices;
pub(crate) use player::posed_player;
pub use textures::{load_or_fallback, procedural_dirt_texture, EntityTextures};
pub(crate) use transform::{body_matrix, x_mirror};

/// Meshes entity snapshots into batched vertex lists grouped by [`TextureSlot`].
#[must_use]
pub fn mesh_entities(
    snapshots: &[EntityRenderSnapshot],
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    let mut batches: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();

    for ent in snapshots {
        let matrix = transform::model_matrix(ent);
        let light = entity_light(ent);
        for layer in mobs::build_entity_layers(ent) {
            let batch = batches.entry(layer.slot).or_default();
            batch.extend(
                layer
                    .vertices
                    .into_iter()
                    .map(|vertex| transform_vertex(vertex, &matrix, light)),
            );
        }
    }

    batches
}

/// Bit of [`EntityVertex::light`] that tints the vertex red, as the hurt and
/// death flash does (read by `entity.wgsl`).
const HURT_BIT: u32 = 1 << 8;

/// The packed light of an entity's vertices.
fn entity_light(entity: &EntityRenderSnapshot) -> u32 {
    if entity.hurt {
        FULL_LIGHT | HURT_BIT
    } else {
        FULL_LIGHT
    }
}

/// Full block and sky light: entities are not lit by the world yet.
const FULL_LIGHT: u32 = 0x00FF_00FF;

fn transform_vertex(mv: ModelVertex, matrix: &Mat4, light: u32) -> EntityVertex {
    EntityVertex {
        position: matrix.transform_point3(Vec3::from(mv.position)).to_array(),
        normal: matrix
            .transform_vector3(Vec3::from(mv.normal))
            .normalize_or_zero()
            .to_array(),
        uv: mv.uv,
        light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId, EntityKind};

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
                swing_progress: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
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
                swing_progress: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
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
                swing_progress: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
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
                swing_progress: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
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
