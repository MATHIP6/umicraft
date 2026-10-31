// SPDX-License-Identifier: MIT OR Apache-2.0

//! A skull worn on the head (MCP `LayerCustomHead` and
//! `TileEntitySkullRenderer.renderSkull`).
//!
//! The helmet slot holds the skull item (id 397); its damage is the type: `0`
//! skeleton, `1` wither skeleton, `2` zombie, `3` player, `4` creeper. The
//! layer scales the skull model by 1.1875 and `renderSkull` turns it back to
//! face forward, so the skull is the head model, 8 pixels cubed (and for the
//! zombie and the player a hat layer 0.25 larger), inflated by 1.1875 about
//! the pivot of the head it sits on.

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{ModelBox, ModelPart, ModelVertex, TextureSlot};

use umicraft_assets::profile_textures::parse_textures_property;
use umicraft_game::item_nbt::ItemDisplay;

use super::skins::SkinLookup;

use super::mobs::{posed_humanoid_mob, EntityLayer};
use super::player::posed_player;

/// The skull item id.
const SKULL: i16 = 397;
/// The damage of a player skull.
const PLAYER_SKULL: i16 = 3;
/// The factor `LayerCustomHead` scales a skull by.
const SKULL_SCALE: f32 = 1.1875;
/// Extra lift of a skull on a villager (`translate(0, 0.0625, 0)` in the scaled frame).
const VILLAGER_LIFT: f32 = 0.0625;

/// The skull layers of `ent`, empty when it wears none or has no head layer.
pub(super) fn skull_layers(ent: &EntityRenderSnapshot, skins: &dyn SkinLookup) -> Vec<EntityLayer> {
    let Some(skull) = ent.armor[3].as_ref().filter(|item| item.item_id == SKULL) else {
        return Vec::new();
    };
    let (head, scale) = match ent.kind {
        EntityKind::Player => (posed_player(ent).0.head, 1.0),
        EntityKind::Zombie | EntityKind::Skeleton | EntityKind::ZombiePigman => {
            match posed_humanoid_mob(ent) {
                Some(mob) => (mob.model.head, mob.scale),
                None => return Vec::new(),
            }
        }
        _ => return Vec::new(),
    };
    let (mut slot, humanoid) = skull_texture(skull.damage);
    if skull.damage == PLAYER_SKULL {
        // The skin of the owner, when the server sent the owner with its
        // textures and the download is ready (`TileEntitySkullRenderer`
        // would ask the session server for a bare name; this does not).
        let owner_skin = skull
            .nbt
            .as_deref()
            .and_then(|nbt| ItemDisplay::from_nbt(nbt).skull_textures)
            .and_then(|value| parse_textures_property(&value).ok())
            .and_then(|textures| textures.skin)
            .and_then(|url| skins.skin(&url));
        if let Some(index) = owner_skin {
            slot = TextureSlot::Skin(index);
        }
    }
    let lift = if ent.meta.is_zombie_villager() {
        VILLAGER_LIFT * SKULL_SCALE
    } else {
        0.0
    };
    let mut vertices = skull_mesh(&head, humanoid, lift);
    scale_mesh(&mut vertices, scale);
    vec![EntityLayer::new(slot, vertices)]
}

/// The texture of a skull type and whether its model is the humanoid head
/// (64 x 64 with a hat layer) rather than the plain skull (64 x 32).
fn skull_texture(skull_type: i16) -> (TextureSlot, bool) {
    match skull_type {
        1 => (TextureSlot::SkeletonWither, false),
        2 => (TextureSlot::Zombie, true),
        // A player skull wears the default skin: the owner's skin needs a
        // profile lookup the client does not make.
        3 => (TextureSlot::Steve, true),
        4 => (TextureSlot::Creeper, false),
        _ => (TextureSlot::Skeleton, false),
    }
}

/// The skull model placed on `head` (MCP `ModelSkeletonHead`,
/// `ModelHumanoidHead`): inflated by [`SKULL_SCALE`] about the pivot of the
/// head and lifted by `lift` blocks.
fn skull_mesh(head: &ModelPart, humanoid: bool, lift: f32) -> Vec<ModelVertex> {
    let texture = if humanoid { [64.0, 64.0] } else { [64.0, 32.0] };
    let mut part = ModelPart::new(head.pivot).with_box(ModelBox::new(
        [-4.0, 0.0, -4.0],
        [8.0, 8.0, 8.0],
        [0.0, 0.0],
        texture,
        0.0,
    ));
    if humanoid {
        part = part.with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0, 8.0, 8.0],
            [32.0, 0.0],
            texture,
            0.25,
        ));
    }
    part.rotation = head.rotation;
    let mut vertices = Vec::new();
    part.emit(Mat4::from_scale(Vec3::splat(1.0 / 16.0)), &mut vertices);
    let pivot = Vec3::from(head.pivot) / 16.0;
    for vertex in &mut vertices {
        let moved = pivot + (Vec3::from(vertex.position) - pivot) * SKULL_SCALE;
        vertex.position = [moved.x, moved.y + lift, moved.z];
    }
    vertices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_render::skins::NoSkins;
    use umicraft_game::entity::{EntityId, EntityItem};

    fn wearing(kind: EntityKind, skull_type: i16) -> EntityRenderSnapshot {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), kind, glam::Vec3::ZERO);
        ent.armor[3] = Some(EntityItem {
            item_id: SKULL,
            damage: skull_type,
            count: 1,
            nbt: None,
        });
        ent
    }

    fn slot_of(ent: &EntityRenderSnapshot) -> Option<TextureSlot> {
        skull_layers(ent, &NoSkins).first().map(|layer| layer.slot)
    }

    /// The NBT of a player skull whose owner carries the textures property
    /// `{"textures":{"SKIN":{"url":"http://t/skin"}}}`.
    fn owner_nbt() -> std::sync::Arc<[u8]> {
        let value = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHA6Ly90L3NraW4ifX19";
        let mut nbt = vec![10, 0, 0, 10, 0, 10];
        nbt.extend(b"SkullOwner");
        nbt.extend([10, 0, 10]);
        nbt.extend(b"Properties");
        nbt.extend([9, 0, 8]);
        nbt.extend(b"textures");
        nbt.extend([10, 0, 0, 0, 1, 8, 0, 5]);
        nbt.extend(b"Value");
        nbt.extend((value.len() as u16).to_be_bytes());
        nbt.extend(value.as_bytes());
        // The texture, Properties, SkullOwner and the root.
        nbt.extend([0, 0, 0, 0]);
        nbt.into()
    }

    #[test]
    fn a_player_skull_wears_the_skin_of_its_owner_once_downloaded() {
        struct Ready;
        impl SkinLookup for Ready {
            fn skin(&self, url: &str) -> Option<u32> {
                (url == "http://t/skin").then_some(7)
            }
            fn cape(&self, _url: &str) -> Option<u32> {
                None
            }
        }
        let mut ent = wearing(EntityKind::Player, 3);
        ent.armor[3].as_mut().unwrap().nbt = Some(owner_nbt());
        assert_eq!(
            skull_layers(&ent, &Ready)[0].slot,
            TextureSlot::Skin(7),
            "the owner skin"
        );
        // Not downloaded yet, or no textures: the default skin.
        assert_eq!(skull_layers(&ent, &NoSkins)[0].slot, TextureSlot::Steve);
        let bare = wearing(EntityKind::Player, 3);
        assert_eq!(skull_layers(&bare, &Ready)[0].slot, TextureSlot::Steve);
    }

    #[test]
    fn each_skull_type_uses_the_texture_of_its_mob() {
        let types = [
            (0, TextureSlot::Skeleton),
            (1, TextureSlot::SkeletonWither),
            (2, TextureSlot::Zombie),
            (3, TextureSlot::Steve),
            (4, TextureSlot::Creeper),
        ];
        for (skull_type, slot) in types {
            assert_eq!(
                slot_of(&wearing(EntityKind::Player, skull_type)),
                Some(slot)
            );
        }
    }

    #[test]
    fn only_a_skull_in_the_helmet_slot_of_a_biped_is_drawn() {
        let mut player = wearing(EntityKind::Player, 0);
        player.armor[3].as_mut().expect("skull").item_id = 306;
        assert!(skull_layers(&player, &NoSkins).is_empty(), "an iron helmet");
        assert!(skull_layers(&wearing(EntityKind::Cow, 0), &NoSkins).is_empty());
        assert!(skull_layers(&wearing(EntityKind::Giant, 0), &NoSkins).is_empty());
        assert!(!skull_layers(&wearing(EntityKind::Skeleton, 0), &NoSkins).is_empty());
        assert!(!skull_layers(&wearing(EntityKind::Zombie, 2), &NoSkins).is_empty());
    }

    #[test]
    fn a_humanoid_skull_has_the_hat_layer_and_the_others_do_not() {
        let boxes = |skull_type| {
            skull_layers(&wearing(EntityKind::Player, skull_type), &NoSkins)[0]
                .vertices
                .len()
                / 36
        };
        assert_eq!(boxes(0), 1);
        assert_eq!(boxes(3), 2);
    }

    #[test]
    fn the_skull_is_a_little_larger_than_the_head_and_sits_on_its_pivot() {
        let player = wearing(EntityKind::Player, 0);
        let head = posed_player(&player).0.head;
        let vertices = &skull_layers(&player, &NoSkins)[0].vertices;
        let width = |axis: usize| {
            let values = vertices.iter().map(|v| v.position[axis]);
            values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
        };
        // 8 pixels scaled by 1.1875 is 0.59375 of a block, a little more than
        // the 0.5 of the head.
        assert!(
            (width(0) - 8.0 * SKULL_SCALE / 16.0).abs() < 0.02,
            "{}",
            width(0)
        );
        let bottom = vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MAX, f32::min);
        assert!((bottom - head.pivot[1] / 16.0).abs() < 0.05, "{bottom}");
    }
}
