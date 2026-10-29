// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity meshing and rendering resources.
//!
//! - [`textures`]: the texture registry and its GPU upload.
//! - [`mobs`]: one builder per mob family, mapping a snapshot to mesh layers.
//! - [`player`]: the posed player model, shared with held items.
//! - `skins`: the downloads of player skins and capes.
//! - [`pose`]: the humanoid pose of a snapshot.
//! - [`transform`]: where a model stands, turns, falls and sleeps.
//! - [`hand`]: the first-person arm.
//!
//! Adding a new mob requires three steps:
//! 1. Add a [`TextureSlot`] variant in `umicraft-model`.
//! 2. Add an entry in `textures::texture_specs` with the jar path and fallback.
//! 3. Add the kind to the matching family builder in [`mobs`].

mod animals;
mod armor;
mod creatures;
mod fire;
mod hand;
mod horse_coat;
mod mobs;
mod mushrooms;
mod player;
mod pose;
mod skins;
mod specs;
mod textures;
mod transform;
mod variants;

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;

pub(crate) use animals::WITCH_SCALE;
pub use hand::hand_vertices;
pub(crate) use mobs::posed_humanoid_mob;
pub(crate) use player::posed_player;
pub use textures::{load_or_fallback, procedural_dirt_texture, EntityTextures};
pub(crate) use transform::{body_matrix, x_mirror};

/// Meshes entity snapshots into batched vertex lists grouped by [`TextureSlot`].
#[must_use]
pub fn mesh_entities(
    snapshots: &[EntityRenderSnapshot],
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    mesh_entities_with(snapshots, &skins::NoSkins, 0.0)
}

/// [`mesh_entities`] with the downloaded skins and capes of players.
fn mesh_entities_with(
    snapshots: &[EntityRenderSnapshot],
    skins: &dyn skins::SkinLookup,
    view_yaw: f32,
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    let mut batches: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();

    for ent in snapshots {
        let matrix = transform::model_matrix(ent);
        let light = entity_light(ent);
        for layer in mobs::build_entity_layers(ent, skins) {
            let light = light | dye_bits(layer.dye);
            let batch = batches.entry(layer.slot).or_default();
            batch.extend(
                layer
                    .vertices
                    .into_iter()
                    .map(|vertex| transform_vertex(vertex, &matrix, light)),
            );
        }
        fire::add_fire(&mut batches, ent, view_yaw);
    }

    batches
}

/// Bit of [`EntityVertex::light`] that tints the vertex red, as the hurt and
/// death flash does (read by `entity.wgsl`).
const HURT_BIT: u32 = 1 << 8;
/// Shift of the 4-bit white flash level (the fusing creeper).
const FLASH_SHIFT: u32 = 9;
/// Bit that enables the dye tint, and the shift of its 4-bit colour.
const DYE_BIT: u32 = 1 << 13;
const DYE_SHIFT: u32 = 14;

/// Full block and sky light: entities are not lit by the world yet.
const FULL_LIGHT: u32 = 0x0000_00FF;

/// The packed light of an entity's vertices: light, hurt tint and creeper
/// flash.
fn entity_light(entity: &EntityRenderSnapshot) -> u32 {
    let hurt = if entity.hurt { HURT_BIT } else { 0 };
    FULL_LIGHT | hurt | (flash_level(entity.fuse_flash) << FLASH_SHIFT)
}

/// The white flash of a creeper as a level `0..=15`: it blinks, white every
/// other tenth of the fuse (MCP `RenderCreeper.getColorMultiplier`).
fn flash_level(fuse_flash: f32) -> u32 {
    if fuse_flash <= 0.0 || (fuse_flash * 10.0) as i32 % 2 == 0 {
        return 0;
    }
    (fuse_flash.min(1.0) * 15.0).round() as u32
}

/// The dye tint bits of a layer.
fn dye_bits(dye: Option<u8>) -> u32 {
    dye.map_or(0, |color| DYE_BIT | (u32::from(color & 15) << DYE_SHIFT))
}

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
mod light_tests {
    use super::*;

    #[test]
    fn the_flash_blinks_and_grows_with_the_fuse() {
        assert_eq!(flash_level(0.0), 0);
        assert_eq!(flash_level(0.05), 0, "the first tenth is dark");
        assert!(flash_level(0.15) > 0, "the second tenth is white");
        assert_eq!(flash_level(0.25), 0);
        assert_eq!(flash_level(0.95), 14);
    }

    #[test]
    fn a_dye_is_packed_above_the_hurt_and_flash_bits() {
        assert_eq!(dye_bits(None), 0);
        assert_eq!(dye_bits(Some(0)), DYE_BIT);
        assert_eq!(dye_bits(Some(14)), DYE_BIT | (14 << DYE_SHIFT));
        let entity = EntityRenderSnapshot::new(
            umicraft_game::entity::EntityId(1),
            umicraft_game::entity::EntityKind::Sheep,
            Vec3::ZERO,
        );
        assert_eq!(entity_light(&entity) & !0xFF, 0);
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
                light: umicraft_world::LightLevel::FULL_SKY,
                position: Vec3::new(10.0, 64.0, 10.0),
                yaw: 0.0,
                body_yaw: 0.0,
                pitch: 0.0,
                head_yaw: 0.0,
                limb_swing: 0.5,
                limb_swing_amount: 0.3,
                swing_progress: 0.0,
                fuse_flash: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
                cape: Default::default(),
                name_tag: None,
                profile: None,
                uuid: Some([0; 16]),
                held: None,
                armor: [None, None, None, None],
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                use_ticks: 0,
                fishing: false,
                flags: EntityFlags::new(0, 0x7F),
            },
            EntityRenderSnapshot {
                id: EntityId(2),
                kind: EntityKind::Zombie,
                light: umicraft_world::LightLevel::FULL_SKY,
                position: Vec3::new(12.0, 64.0, 10.0),
                yaw: 90.0,
                body_yaw: 90.0,
                pitch: 0.0,
                head_yaw: 90.0,
                limb_swing: 0.0,
                limb_swing_amount: 0.0,
                swing_progress: 0.0,
                fuse_flash: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
                cape: Default::default(),
                name_tag: None,
                profile: None,
                uuid: None,
                held: None,
                armor: [None, None, None, None],
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                use_ticks: 0,
                fishing: false,
                flags: EntityFlags::default(),
            },
            EntityRenderSnapshot {
                id: EntityId(3),
                kind: EntityKind::Cow,
                light: umicraft_world::LightLevel::FULL_SKY,
                position: Vec3::new(14.0, 64.0, 10.0),
                yaw: 180.0,
                body_yaw: 180.0,
                pitch: 0.0,
                head_yaw: 180.0,
                limb_swing: 1.0,
                limb_swing_amount: 0.5,
                swing_progress: 0.0,
                fuse_flash: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
                cape: Default::default(),
                name_tag: None,
                profile: None,
                uuid: None,
                held: None,
                armor: [None, None, None, None],
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                use_ticks: 0,
                fishing: false,
                flags: EntityFlags::default(),
            },
            // A dropped item has no entity mesh: P3 draws it from its item model.
            EntityRenderSnapshot {
                id: EntityId(4),
                kind: EntityKind::Item,
                light: umicraft_world::LightLevel::FULL_SKY,
                position: Vec3::new(16.0, 64.25, 10.0),
                yaw: 45.0,
                body_yaw: 45.0,
                pitch: 0.0,
                head_yaw: 45.0,
                limb_swing: 0.0,
                limb_swing_amount: 0.0,
                swing_progress: 0.0,
                fuse_flash: 0.0,
                hurt: false,
                death_time: 0.0,
                bed: None,
                meta: Default::default(),
                cape: Default::default(),
                name_tag: None,
                profile: None,
                uuid: None,
                held: None,
                armor: [None, None, None, None],
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                use_ticks: 0,
                fishing: false,
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
