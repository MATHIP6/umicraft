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
use super::armor::armor_layers;
use super::arrows::arrow_layers;
use super::creatures::{creature_layers, creeper_layers};
use super::glow::Glow;
use super::player::posed_player;
use super::pose::humanoid_pose;
use super::skins::SkinLookup;
use super::skull::skull_layers;

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
    /// How the layer glows, when it is added over the frame instead of lit.
    pub(super) glow: Option<Glow>,
    /// Drawn at 15 % opacity: the model of an invisible entity a spectator
    /// sees.
    pub(super) translucent: bool,
    /// Texels whose alpha is under `cutoff / 31` are not drawn: the dissolving
    /// of a dying dragon (`0` keeps the usual alpha test).
    pub(super) cutoff: u8,
}

impl EntityLayer {
    /// An untinted layer.
    pub(super) fn new(slot: TextureSlot, vertices: Vec<ModelVertex>) -> Self {
        Self {
            slot,
            vertices,
            dye: None,
            glow: None,
            translucent: false,
            cutoff: 0,
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
/// entity model, such as dropped items), for a viewer who is not a spectator.
#[cfg(test)]
pub(super) fn build_entity_layers(
    ent: &EntityRenderSnapshot,
    skins: &dyn SkinLookup,
) -> Vec<EntityLayer> {
    build_entity_layers_for(ent, skins, false)
}

/// [`build_entity_layers`] for a viewer who is, or is not, a spectator: a
/// spectator sees the model of an invisible entity at 15 % opacity.
pub(super) fn build_entity_layers_for(
    ent: &EntityRenderSnapshot,
    skins: &dyn SkinLookup,
    viewer_is_spectator: bool,
) -> Vec<EntityLayer> {
    let pose = Pose::of(ent);
    let mut layers = if ent.kind == EntityKind::Creeper {
        creeper_layers(ent, pose)
    } else {
        humanoid_layers(ent, skins)
            .or_else(|| animal_layers(ent, pose))
            .or_else(|| creature_layers(ent, pose))
            .unwrap_or_default()
    };
    // An invisible entity draws no body, only what it wears (MCP
    // `RendererLivingEntity.doRender` skips `renderModel` but not the layers).
    // A spectator sees the main model, the first layer, as a ghost
    // (`isInvisibleToPlayer`).
    if ent.flags.is_invisible() {
        let sees_ghost = viewer_is_spectator || ent.ghost_visible;
        let ghost = (sees_ghost && !layers.is_empty()).then(|| layers.remove(0));
        layers.retain(|layer| layer.slot.is_armor() || layer.slot == TextureSlot::EnchantGlint);
        if let Some(mut body) = ghost {
            body.translucent = true;
            layers.insert(0, body);
        }
    }
    // A skull on the head is drawn even when its wearer is invisible.
    layers.extend(skull_layers(ent));
    // Arrows in the body are drawn on an invisible player too.
    layers.extend(arrow_layers(ent));
    layers
}

/// Two-legged, two-armed models.
fn humanoid_layers(ent: &EntityRenderSnapshot, skins: &dyn SkinLookup) -> Option<Vec<EntityLayer>> {
    if ent.kind == EntityKind::Player {
        let (model, is_slim) = posed_player(ent);
        let slot = skin_slot(ent, is_slim, skins);
        let mut layers = one(slot, model.emit_mesh());
        layers.extend(armor_layers(ent, 1.0));
        layers.extend(cape_layer(ent, skins));
        return Some(layers);
    }
    let HumanoidMob { model, slot, scale } = posed_humanoid_mob(ent)?;
    let mut vertices = if ent.meta.baby {
        model.emit_child_mesh()
    } else {
        model.emit_mesh()
    };
    scale_mesh(&mut vertices, scale);
    let mut layers = one(slot, vertices);
    layers.extend(armor_layers(ent, scale));
    Some(layers)
}

/// The posed model of a zombie-like or skeleton-like mob.
pub(crate) struct HumanoidMob {
    /// The model with the pose of the snapshot applied.
    pub(crate) model: HumanoidModel,
    /// The texture it is drawn with.
    pub(crate) slot: TextureSlot,
    /// The uniform scale of the entity (`preRenderCallback`).
    pub(crate) scale: f32,
}

/// The humanoid model of a mob that wears the biped skeleton, or `None` for
/// every other kind (players have [`posed_player`]). Shared by the body mesh
/// and by the held item, which hangs on the posed right arm.
pub(crate) fn posed_humanoid_mob(ent: &EntityRenderSnapshot) -> Option<HumanoidMob> {
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
    Some(HumanoidMob { model, slot, scale })
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
        assert!(slots(&invisible, &Ready).is_empty(), "no body, no cape");
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

    #[test]
    fn an_invisible_entity_keeps_only_its_armour() {
        use umicraft_game::entity::EntityItem;
        let mut zombie =
            EntityRenderSnapshot::new(EntityId(5), EntityKind::Zombie, glam::Vec3::ZERO);
        zombie.flags = EntityFlags::new(0x20, 0x7F);
        assert!(slots(&zombie, &Ready).is_empty(), "no body, nothing worn");
        zombie.armor[3] = Some(EntityItem {
            item_id: 310,
            damage: 0,
            count: 1,
            nbt: None,
        });
        let kept = slots(&zombie, &Ready);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].is_armor());
        let mut cow = EntityRenderSnapshot::new(EntityId(6), EntityKind::Cow, glam::Vec3::ZERO);
        cow.flags = EntityFlags::new(0x20, 0x7F);
        assert!(slots(&cow, &Ready).is_empty());
    }

    #[test]
    fn a_saddled_pig_draws_its_saddle_over_the_body() {
        let mut pig = EntityRenderSnapshot::new(EntityId(7), EntityKind::Pig, glam::Vec3::ZERO);
        assert_eq!(slots(&pig, &Ready), vec![TextureSlot::Pig]);
        pig.meta.variant = Variant::Pig { saddled: true };
        assert_eq!(
            slots(&pig, &Ready),
            vec![TextureSlot::Pig, TextureSlot::PigSaddle]
        );
    }

    #[test]
    fn a_ghast_shows_the_shooting_face_while_it_attacks() {
        let mut ghast = EntityRenderSnapshot::new(EntityId(8), EntityKind::Ghast, glam::Vec3::ZERO);
        assert_eq!(slots(&ghast, &Ready), vec![TextureSlot::Ghast]);
        ghast.meta.variant = Variant::Ghast { attacking: true };
        assert_eq!(slots(&ghast, &Ready), vec![TextureSlot::GhastShooting]);
    }

    #[test]
    fn a_sitting_wolf_and_ocelot_change_their_pose() {
        use umicraft_game::entity::meta::{OcelotMeta, WolfMeta};
        let vertices = |ent: &EntityRenderSnapshot| {
            build_entity_layers(ent, &Ready)
                .into_iter()
                .flat_map(|layer| layer.vertices)
                .map(|v| v.position)
                .collect::<Vec<_>>()
        };
        let mut wolf = EntityRenderSnapshot::new(EntityId(9), EntityKind::Wolf, glam::Vec3::ZERO);
        let standing = vertices(&wolf);
        wolf.meta.variant = Variant::Wolf(WolfMeta {
            sitting: true,
            ..WolfMeta::default()
        });
        assert_ne!(vertices(&wolf), standing);
        let mut cat = EntityRenderSnapshot::new(EntityId(10), EntityKind::Ocelot, glam::Vec3::ZERO);
        let standing = vertices(&cat);
        cat.meta.variant = Variant::Ocelot(OcelotMeta {
            sitting: true,
            ..OcelotMeta::default()
        });
        assert_ne!(vertices(&cat), standing);
    }

    /// NBT of `{ench: [{id: 16, lvl: 1}]}`.
    const ENCHANTED: &[u8] = &[
        10, 0, 0, 9, 0, 4, b'e', b'n', b'c', b'h', 10, 0, 0, 0, 1, 2, 0, 2, b'i', b'd', 0, 16, 2,
        0, 3, b'l', b'v', b'l', 0, 1, 0, 0,
    ];

    fn glowing_slots(entity: &EntityRenderSnapshot) -> Vec<TextureSlot> {
        build_entity_layers(entity, &Ready)
            .into_iter()
            .filter(|layer| layer.glow.is_some())
            .map(|layer| layer.slot)
            .collect()
    }

    #[test]
    fn spiders_endermen_and_the_dragon_glow_with_their_eyes() {
        for (kind, eyes) in [
            (EntityKind::Spider, TextureSlot::SpiderEyes),
            (EntityKind::CaveSpider, TextureSlot::SpiderEyes),
            (EntityKind::Enderman, TextureSlot::EndermanEyes),
            (EntityKind::Dragon, TextureSlot::DragonEyes),
        ] {
            let ent = EntityRenderSnapshot::new(EntityId(11), kind, glam::Vec3::ZERO);
            assert_eq!(glowing_slots(&ent), vec![eyes], "{kind:?}");
        }
    }

    #[test]
    fn only_a_charged_creeper_wears_the_swirl_and_it_moves_with_the_age() {
        use umicraft_game::entity::meta::CreeperMeta;
        let mut creeper =
            EntityRenderSnapshot::new(EntityId(12), EntityKind::Creeper, glam::Vec3::ZERO);
        assert!(glowing_slots(&creeper).is_empty());
        creeper.meta.variant = Variant::Creeper(CreeperMeta {
            powered: true,
            ..CreeperMeta::default()
        });
        assert_eq!(glowing_slots(&creeper), vec![TextureSlot::CreeperCharge]);
        let mut shift = |age: f32| {
            creeper.age = age;
            build_entity_layers(&creeper, &Ready)
                .into_iter()
                .find_map(|layer| layer.glow)
                .expect("glow")
                .uv
                .shift
        };
        assert_eq!(shift(0.0), [0.0, 0.0]);
        assert_eq!(shift(100.0), [1.0, 1.0]);
    }

    #[test]
    fn a_wither_shows_its_aura_at_half_health_and_is_twice_as_large() {
        let mut wither =
            EntityRenderSnapshot::new(EntityId(13), EntityKind::Wither, glam::Vec3::ZERO);
        wither.meta.living.health = 300.0;
        assert!(glowing_slots(&wither).is_empty());
        wither.meta.living.health = 150.0;
        assert_eq!(glowing_slots(&wither), vec![TextureSlot::WitherAura]);
        let height = |ent: &EntityRenderSnapshot| {
            build_entity_layers(ent, &Ready)
                .into_iter()
                .next()
                .expect("body")
                .vertices
                .iter()
                .map(|v| v.position[1])
                .fold(f32::MIN, f32::max)
        };
        // The model is under 2 blocks tall; scaled by 2 it passes 2.
        assert!(height(&wither) > 2.0);
    }

    #[test]
    fn enchanted_armour_glints_in_two_passes_and_an_invisible_wearer_keeps_them() {
        use umicraft_game::entity::EntityItem;
        let mut player = player(0x7F);
        player.armor[3] = Some(EntityItem {
            item_id: 306,
            damage: 0,
            count: 1,
            nbt: Some(std::sync::Arc::from(ENCHANTED)),
        });
        let glints = glowing_slots(&player);
        assert_eq!(glints, vec![TextureSlot::EnchantGlint; 2]);
        let equal: Vec<bool> = build_entity_layers(&player, &Ready)
            .into_iter()
            .filter_map(|layer| layer.glow.map(|glow| glow.equal_depth))
            .collect();
        assert_eq!(equal, vec![true, true], "only where the armour is");
        player.flags = EntityFlags::new(0x20, 0x7F);
        assert_eq!(glowing_slots(&player), glints);
        // A plain piece does not glint.
        player.armor[3].as_mut().expect("helmet").nbt = None;
        assert!(glowing_slots(&player).is_empty());
    }

    #[test]
    fn a_spectator_sees_the_model_of_an_invisible_entity_as_a_ghost() {
        use umicraft_game::entity::EntityItem;
        let mut zombie =
            EntityRenderSnapshot::new(EntityId(14), EntityKind::Zombie, glam::Vec3::ZERO);
        zombie.flags = EntityFlags::new(0x20, 0x7F);
        zombie.armor[3] = Some(EntityItem {
            item_id: 306,
            damage: 0,
            count: 1,
            nbt: None,
        });
        // Anyone else sees only the armour.
        let others = build_entity_layers_for(&zombie, &Ready, false);
        assert!(others.iter().all(|layer| !layer.translucent));
        assert_eq!(others.len(), 1);
        // A spectator also sees the body, translucent, under the armour.
        let ghosts = build_entity_layers_for(&zombie, &Ready, true);
        assert_eq!(ghosts.len(), 2);
        assert!(ghosts[0].translucent && ghosts[0].slot == TextureSlot::Zombie);
        assert!(!ghosts[1].translucent && ghosts[1].slot.is_armor());
        // A visible entity is never a ghost.
        zombie.flags = EntityFlags::new(0, 0x7F);
        assert!(build_entity_layers_for(&zombie, &Ready, true)
            .iter()
            .all(|layer| !layer.translucent));
    }

    #[test]
    fn an_invisible_creeper_is_hidden_too() {
        let mut creeper =
            EntityRenderSnapshot::new(EntityId(15), EntityKind::Creeper, glam::Vec3::ZERO);
        creeper.flags = EntityFlags::new(0x20, 0x7F);
        assert!(build_entity_layers_for(&creeper, &Ready, false).is_empty());
        assert_eq!(build_entity_layers_for(&creeper, &Ready, true).len(), 1);
    }

    #[test]
    fn a_player_in_spectator_mode_is_only_a_head_and_its_hat() {
        let mut player = player(0x7F);
        let boxes =
            |ent: &EntityRenderSnapshot| build_entity_layers(ent, &Ready)[0].vertices.len() / 36;
        let whole = boxes(&player);
        player.spectator = true;
        assert_eq!(boxes(&player), 2, "the head and the hat of {whole} boxes");
        assert!(whole > 2);
    }
}
