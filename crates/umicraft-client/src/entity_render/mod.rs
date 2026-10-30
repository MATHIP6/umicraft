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
mod arrows;
mod creatures;
mod fire;
mod glow;
mod hand;
mod horse_coat;
mod mobs;
mod mushrooms;
mod paintings;
mod player;
mod pose;
mod shadow;
mod skins;
mod skull;
mod small_models;
mod specs;
mod textures;
mod transform;
mod variants;
mod vehicles;
mod xp;

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;

pub(crate) use animals::WITCH_SCALE;
pub use hand::hand_vertices;
pub(crate) use mobs::posed_humanoid_mob;
pub(crate) use player::posed_player;
pub use textures::{load_or_fallback, procedural_dirt_texture, EntityTextures};
pub(crate) use transform::{body_matrix, part_frame, x_mirror};

/// Meshes entity snapshots into batched vertex lists grouped by [`TextureSlot`].
#[must_use]
pub fn mesh_entities(
    snapshots: &[EntityRenderSnapshot],
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    mesh_entities_with(snapshots, &skins::NoSkins, (0.0, 0.0), false, Vec3::ZERO).solid
}

/// The meshes of a frame: the lit entities, and the glowing layers that are
/// added over them.
struct MeshedEntities {
    solid: HashMap<TextureSlot, Vec<EntityVertex>>,
    additive: HashMap<TextureSlot, Vec<EntityVertex>>,
    /// The glint of armour, drawn only where the armour is.
    glint: HashMap<TextureSlot, Vec<EntityVertex>>,
    /// The invisible entities a spectator sees, drawn translucent.
    ghost: HashMap<TextureSlot, Vec<EntityVertex>>,
    /// The shadows under the entities.
    shadows: HashMap<TextureSlot, Vec<EntityVertex>>,
}

/// [`mesh_entities`] with the downloaded skins and capes of players.
fn mesh_entities_with(
    snapshots: &[EntityRenderSnapshot],
    skins: &dyn skins::SkinLookup,
    (view_yaw, view_pitch): (f32, f32),
    viewer_is_spectator: bool,
    camera: Vec3,
) -> MeshedEntities {
    let mut solid: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();
    let mut additive: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();
    let mut glint: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();
    let mut ghost: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();
    let mut shadows: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();

    for ent in snapshots {
        let matrix = transform::model_matrix(ent);
        let light = entity_light(ent);
        for layer in mobs::build_entity_layers_for(ent, skins, viewer_is_spectator) {
            if let Some(glow) = layer.glow {
                let word = glow.light_word();
                let target = if glow.equal_depth {
                    &mut glint
                } else {
                    &mut additive
                };
                target
                    .entry(layer.slot)
                    .or_default()
                    .extend(layer.vertices.into_iter().map(|mut vertex| {
                        vertex.uv = glow.uv.apply(vertex.uv);
                        transform_vertex(vertex, &matrix, word)
                    }));
                continue;
            }
            let light = light | dye_bits(layer.dye);
            let target = if layer.translucent {
                &mut ghost
            } else {
                &mut solid
            };
            target.entry(layer.slot).or_default().extend(
                layer
                    .vertices
                    .into_iter()
                    .map(|vertex| transform_vertex(vertex, &matrix, light)),
            );
        }
        fire::add_fire(&mut solid, ent, view_yaw);
        arrows::add_flying_arrow(&mut solid, ent, light);
        vehicles::add_vehicle(&mut solid, ent, light);
        xp::add_xp_orb(&mut solid, ent, light, view_yaw, view_pitch);
        paintings::add_painting(&mut solid, ent, light);
        small_models::add_small_model(&mut solid, ent, light);
        shadow::add_shadow(&mut shadows, ent, camera);
    }

    MeshedEntities {
        solid,
        additive,
        glint,
        ghost,
        shadows,
    }
}

/// Bit of [`EntityVertex::light`] that tints the vertex red, as the hurt and
/// death flash does (read by `entity.wgsl`).
const HURT_BIT: u32 = 1 << 8;
/// Shift of the 4-bit white flash level (the fusing creeper).
const FLASH_SHIFT: u32 = 9;
/// Bit that enables the dye tint, and the shift of its 4-bit colour.
const DYE_BIT: u32 = 1 << 13;
const DYE_SHIFT: u32 = 14;

/// Full block and sky light (`15728880` in the reference).
const FULL_LIGHT: u32 = 0x0000_00FF;

/// Whether the entity ignores the world light: the wither, the blaze and the
/// magma cube return the full light from `getBrightnessForRender`.
fn lights_itself(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Wither | EntityKind::Blaze | EntityKind::MagmaCube
    )
}

/// The block and sky light nibbles of an entity's vertices: the light of the
/// block it stands in (MCP `Entity.getBrightnessForRender`).
fn world_light_bits(entity: &EntityRenderSnapshot) -> u32 {
    if lights_itself(entity.kind) {
        return FULL_LIGHT;
    }
    u32::from(entity.light.block.min(15)) | (u32::from(entity.light.sky.min(15)) << 4)
}

/// The packed light of an entity's vertices: light, hurt tint and creeper
/// flash.
fn entity_light(entity: &EntityRenderSnapshot) -> u32 {
    let hurt = if entity.hurt { HURT_BIT } else { 0 };
    world_light_bits(entity) | hurt | (flash_level(entity.fuse_flash) << FLASH_SHIFT)
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
mod world_light_tests {
    use super::*;
    use umicraft_game::entity::EntityId;
    use umicraft_world::LightLevel;

    fn entity(kind: EntityKind, light: LightLevel) -> EntityRenderSnapshot {
        let mut entity = EntityRenderSnapshot::new(EntityId(1), kind, Vec3::ZERO);
        entity.light = light;
        entity
    }

    #[test]
    fn a_mob_is_lit_by_the_block_it_stands_in() {
        let dark = entity(EntityKind::Zombie, LightLevel::DARK);
        assert_eq!(entity_light(&dark) & 0xFF, 0);
        let lamp = entity(EntityKind::Zombie, LightLevel { block: 7, sky: 3 });
        assert_eq!(entity_light(&lamp) & 0xFF, 7 | (3 << 4));
        let day = entity(EntityKind::Zombie, LightLevel::FULL_SKY);
        assert_eq!(entity_light(&day) & 0xFF, 15 << 4);
    }

    #[test]
    fn the_wither_the_blaze_and_the_magma_cube_ignore_the_world_light() {
        for kind in [EntityKind::Wither, EntityKind::Blaze, EntityKind::MagmaCube] {
            assert_eq!(entity_light(&entity(kind, LightLevel::DARK)) & 0xFF, 0xFF);
        }
        assert_eq!(
            entity_light(&entity(EntityKind::Creeper, LightLevel::DARK)) & 0xFF,
            0
        );
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
                spectator: false,
                object_data: 0,
                shadow: Default::default(),
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
                spectator: false,
                object_data: 0,
                shadow: Default::default(),
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
                spectator: false,
                object_data: 0,
                shadow: Default::default(),
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
                spectator: false,
                object_data: 0,
                shadow: Default::default(),
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
