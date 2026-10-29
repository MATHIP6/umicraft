// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mesh builders for every mob, grouped by model family.
//!
//! The families live in `animals` and `creatures`, the humanoids here. Each
//! family function returns `None` for kinds it does not own, so
//! [`build_entity_layers`] chains them. Each layer is tagged with a
//! [`TextureSlot`] so the batching loop routes vertices to the right texture,
//! and with the dye that tints it, if any.

use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{CapeModel, HumanoidModel, ModelVertex, TextureSlot};

use super::animals::animal_layers;
use super::creatures::{creature_layers, creeper_layers};
use super::player::posed_player;
use super::pose::humanoid_pose;
use super::skins::SkinLookup;

/// The animation inputs every model's `set_angles` takes.
#[derive(Clone, Copy)]
pub(super) struct Pose {
    pub(super) swing: f32,
    pub(super) amount: f32,
    /// Head yaw relative to the body, in degrees.
    pub(super) head_yaw: f32,
    pub(super) head_pitch: f32,
}

impl Pose {
    fn of(ent: &EntityRenderSnapshot) -> Self {
        Self {
            swing: ent.limb_swing,
            amount: ent.limb_swing_amount,
            head_yaw: ent.head_yaw - ent.body_yaw,
            head_pitch: ent.pitch,
        }
    }
}

/// A mesh with its texture and an optional dye tint.
pub(super) struct EntityLayer {
    /// The texture the vertices use.
    pub(super) slot: TextureSlot,
    /// Model-space vertices.
    pub(super) vertices: Vec<ModelVertex>,
    /// Dye (`0..=15`, MCP `EnumDyeColor` metadata) that multiplies the
    /// texture: the sheep wool and the wolf collar.
    pub(super) dye: Option<u8>,
}

impl EntityLayer {
    /// An untinted layer.
    pub(super) fn new(slot: TextureSlot, vertices: Vec<ModelVertex>) -> Self {
        Self {
            slot,
            vertices,
            dye: None,
        }
    }

    /// The layer tinted with the dye `color`.
    pub(super) fn dyed(mut self, color: u8) -> Self {
        self.dye = Some(color & 15);
        self
    }
}

/// A single-layer mesh on `slot`.
pub(super) fn one(slot: TextureSlot, vertices: Vec<ModelVertex>) -> Vec<EntityLayer> {
    vec![EntityLayer::new(slot, vertices)]
}

/// Builds mesh layers for a single entity snapshot (empty for kinds with no
/// entity model, such as dropped items).
pub(super) fn build_entity_layers(
    ent: &EntityRenderSnapshot,
    skins: &dyn SkinLookup,
) -> Vec<EntityLayer> {
    let pose = Pose::of(ent);
    if ent.kind == EntityKind::Creeper {
        return creeper_layers(ent, pose);
    }
    humanoid_layers(ent, skins)
        .or_else(|| animal_layers(ent, pose))
        .or_else(|| creature_layers(ent, pose))
        .unwrap_or_default()
}

/// Two-legged, two-armed models.
fn humanoid_layers(ent: &EntityRenderSnapshot, skins: &dyn SkinLookup) -> Option<Vec<EntityLayer>> {
    if ent.kind == EntityKind::Player {
        let (model, is_slim) = posed_player(ent);
        let slot = skin_slot(ent, is_slim, skins);
        let mut layers = one(slot, model.emit_mesh());
        layers.extend(cape_layer(ent, skins));
        return Some(layers);
    }
    let wither = matches!(ent.meta.variant, Variant::Skeleton { wither: true });
    let (mut model, slot, scale) = match ent.kind {
        EntityKind::Zombie if ent.meta.is_zombie_villager() => (
            HumanoidModel::zombie_villager(),
            TextureSlot::ZombieVillager,
            1.0,
        ),
        EntityKind::Zombie => (HumanoidModel::zombie(), TextureSlot::Zombie, 1.0),
        EntityKind::Skeleton if wither => (
            HumanoidModel::skeleton(),
            TextureSlot::SkeletonWither,
            WITHER_SKELETON_SCALE,
        ),
        EntityKind::Skeleton => (HumanoidModel::skeleton(), TextureSlot::Skeleton, 1.0),
        EntityKind::ZombiePigman => (HumanoidModel::zombie(), TextureSlot::ZombiePigman, 1.0),
        EntityKind::Giant => (HumanoidModel::zombie(), TextureSlot::Zombie, GIANT_SCALE),
        EntityKind::ZombieVillager => (
            HumanoidModel::zombie_villager(),
            TextureSlot::ZombieVillager,
            1.0,
        ),
        _ => return None,
    };
    // `ModelSkeleton` extends `ModelZombie`: all of them hold the arms forward.
    model.set_pose(&humanoid_pose(ent, true));
    let mut vertices = if ent.meta.baby {
        model.emit_child_mesh()
    } else {
        model.emit_mesh()
    };
    scale_mesh(&mut vertices, scale);
    Some(one(slot, vertices))
}

/// Scale of a wither skeleton (MCP `RenderSkeleton.preRenderCallback`).
const WITHER_SKELETON_SCALE: f32 = 1.2;
/// Scale of a giant (MCP `RenderGiantZombie`).
const GIANT_SCALE: f32 = 6.0;

/// The texture of a player: the downloaded skin, or the default skin that
/// matches the arm model.
fn skin_slot(ent: &EntityRenderSnapshot, is_slim: bool, skins: &dyn SkinLookup) -> TextureSlot {
    let downloaded = ent
        .profile
        .as_ref()
        .and_then(|profile| profile.skin_url.as_deref())
        .and_then(|url| skins.skin(url));
    match downloaded {
        Some(index) => TextureSlot::Skin(index),
        None if is_slim => TextureSlot::Alex,
        None => TextureSlot::Steve,
    }
}

/// The cape of a player, when it is enabled, downloaded and the player is
/// visible (MCP `LayerCape.doRenderLayer`).
fn cape_layer(ent: &EntityRenderSnapshot, skins: &dyn SkinLookup) -> Option<EntityLayer> {
    let enabled = ent.flags.skin_parts & CAPE_PART != 0 && !ent.flags.is_invisible();
    let index = ent
        .profile
        .as_ref()
        .and_then(|profile| profile.cape_url.as_deref())
        .filter(|_| enabled)
        .and_then(|url| skins.cape(url))?;
    let mesh = CapeModel::new().emit_mesh(ent.cape.pitch, ent.cape.roll, ent.flags.is_sneaking());
    Some(EntityLayer::new(TextureSlot::Cape(index), mesh))
}

/// Bit of the skin parts that shows the cape (MCP `EnumPlayerModelParts.CAPE`).
const CAPE_PART: u8 = 0x01;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use umicraft_game::entity::{EntityFlags, EntityId, PlayerProfile};

    /// A lookup where every texture is ready with index 3.
    struct Ready;

    impl SkinLookup for Ready {
        fn skin(&self, _url: &str) -> Option<u32> {
            Some(3)
        }

        fn cape(&self, _url: &str) -> Option<u32> {
            Some(3)
        }
    }

    fn player(skin_parts: u8) -> EntityRenderSnapshot {
        let mut entity =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Player, glam::Vec3::ZERO);
        entity.flags = EntityFlags::new(0, skin_parts);
        let mut profile = PlayerProfile::named("Steve");
        profile.skin_url = Some(Arc::from("http://textures.minecraft.net/texture/ab"));
        profile.cape_url = Some(Arc::from("http://textures.minecraft.net/texture/cd"));
        entity.profile = Some(Arc::new(profile));
        entity
    }

    fn slots(entity: &EntityRenderSnapshot, skins: &dyn SkinLookup) -> Vec<TextureSlot> {
        build_entity_layers(entity, skins)
            .into_iter()
            .map(|layer| layer.slot)
            .collect()
    }

    #[test]
    fn a_ready_skin_and_cape_are_used() {
        assert_eq!(
            slots(&player(0x7F), &Ready),
            vec![TextureSlot::Skin(3), TextureSlot::Cape(3)]
        );
    }

    #[test]
    fn a_skin_that_is_not_ready_falls_back_to_the_default_skin() {
        let slots = slots(&player(0x7F), &super::super::skins::NoSkins);
        assert_eq!(slots.len(), 1);
        assert!(matches!(slots[0], TextureSlot::Steve | TextureSlot::Alex));
    }

    #[test]
    fn the_cape_part_and_invisibility_hide_the_cape() {
        assert_eq!(slots(&player(0x7E), &Ready), vec![TextureSlot::Skin(3)]);
        let mut invisible = player(0x7F);
        invisible.flags = EntityFlags::new(0x20, 0x7F);
        assert_eq!(slots(&invisible, &Ready), vec![TextureSlot::Skin(3)]);
    }

    #[test]
    fn the_profile_decides_the_arm_model_of_the_fallback() {
        let mut slim = player(0x7F);
        let mut profile = PlayerProfile::named("Alex");
        profile.slim = Some(true);
        slim.profile = Some(Arc::new(profile));
        assert_eq!(
            slots(&slim, &super::super::skins::NoSkins),
            vec![TextureSlot::Alex]
        );
    }
}
