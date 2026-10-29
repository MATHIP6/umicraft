// SPDX-License-Identifier: MIT OR Apache-2.0

//! Worn armour of players and armoured mobs (MCP `LayerBipedArmor`,
//! `LayerArmorBase`).
//!
//! Each worn piece is a shell of the biped model, posed like the body, drawn
//! with the texture of its material. Leather is multiplied by its dye colour
//! (`display.color`) and gets a second, undyed overlay pass.

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_game::item_nbt::ItemDisplay;
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{ArmorMaterial, HumanoidModel, TextureSlot};

use super::mobs::EntityLayer;
use super::pose::humanoid_pose;

/// The colour of undyed leather armour (MCP `ItemArmor.getColor`, 10511680).
const DEFAULT_LEATHER: u32 = 0x00A0_6540;

/// The armour layers of `ent`, helmet first. `scale` is the entity scale of
/// the mob wearing it.
pub(super) fn armor_layers(ent: &EntityRenderSnapshot, scale: f32) -> Vec<EntityLayer> {
    if !wears_armor(ent.kind) {
        return Vec::new();
    }
    let pose = humanoid_pose(ent, ent.kind != EntityKind::Player);
    let mut layers = Vec::new();
    for slot in (1..=4u8).rev() {
        let Some(item) = &ent.armor[usize::from(slot) - 1] else {
            continue;
        };
        let Some(material) = ArmorMaterial::from_item_id(item.item_id) else {
            continue;
        };
        let mut model = HumanoidModel::armor(slot);
        model.set_pose(&pose);
        let mut vertices = if ent.meta.baby {
            model.emit_child_mesh()
        } else {
            model.emit_mesh()
        };
        scale_mesh(&mut vertices, scale);
        let leggings = slot == 2;
        if material == ArmorMaterial::Leather {
            let color = item
                .nbt
                .as_deref()
                .map(ItemDisplay::from_nbt)
                .and_then(|display| display.leather_color)
                .unwrap_or(DEFAULT_LEATHER);
            layers.push(EntityLayer::new(
                TextureSlot::LeatherArmor { leggings, color },
                vertices.clone(),
            ));
            layers.push(EntityLayer::new(
                TextureSlot::LeatherOverlay { leggings },
                vertices,
            ));
        } else {
            layers.push(EntityLayer::new(
                TextureSlot::Armor { material, leggings },
                vertices,
            ));
        }
    }
    layers
}

/// The kinds whose renderer has a `LayerBipedArmor`.
fn wears_armor(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Player
            | EntityKind::Zombie
            | EntityKind::Skeleton
            | EntityKind::ZombiePigman
            | EntityKind::Giant
    )
}

/// The jar path of an armour texture.
fn path(name: &str, leggings: bool, suffix: &str) -> String {
    let layer = if leggings { 2 } else { 1 };
    format!("assets/minecraft/textures/models/armor/{name}_layer_{layer}{suffix}.png")
}

/// The pixels of the armour texture of `slot`, or `None` for another slot
/// and when the jar has no such texture (the piece is then not drawn: an
/// original stand-in cannot reproduce the official art).
pub(super) fn armor_texture(
    jar: Option<&ClientJar>,
    slot: TextureSlot,
) -> Option<(Vec<u8>, u32, u32)> {
    let (name, leggings, suffix, color) = match slot {
        TextureSlot::Armor { material, leggings } => (material.texture_name(), leggings, "", None),
        TextureSlot::LeatherArmor { leggings, color } => ("leather", leggings, "", Some(color)),
        TextureSlot::LeatherOverlay { leggings } => ("leather", leggings, "_overlay", None),
        _ => return None,
    };
    let bytes = jar?.read_entry(&path(name, leggings, suffix)).ok()?;
    let (mut rgba, w, h) = decode_png_bytes(&bytes).ok()?;
    if let Some(color) = color {
        tint(&mut rgba, color);
    }
    Some((rgba, w, h))
}

/// Multiplies the colour channels by `0xRRGGBB`, as `GlStateManager.color`
/// does for the leather pass.
fn tint(rgba: &mut [u8], color: u32) {
    let channels = [(color >> 16) & 255, (color >> 8) & 255, color & 255];
    for pixel in rgba.chunks_exact_mut(4) {
        for (value, factor) in pixel.iter_mut().zip(channels) {
            *value = (u32::from(*value) * factor / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityId, EntityItem};

    fn item(item_id: i16) -> Option<EntityItem> {
        Some(EntityItem {
            item_id,
            damage: 0,
            count: 1,
            nbt: None,
        })
    }

    fn player() -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), EntityKind::Player, glam::Vec3::ZERO)
    }

    fn slots(ent: &EntityRenderSnapshot) -> Vec<TextureSlot> {
        armor_layers(ent, 1.0).iter().map(|l| l.slot).collect()
    }

    #[test]
    fn a_bare_player_wears_nothing() {
        assert!(armor_layers(&player(), 1.0).is_empty());
    }

    #[test]
    fn each_piece_uses_its_material_and_the_leggings_texture_is_layer_two() {
        let mut ent = player();
        ent.armor = [item(309), item(308), item(307), item(306)];
        let iron = |leggings| TextureSlot::Armor {
            material: ArmorMaterial::Iron,
            leggings,
        };
        assert_eq!(
            slots(&ent),
            vec![iron(false), iron(false), iron(true), iron(false)]
        );
    }

    #[test]
    fn leather_gets_a_dyed_pass_and_an_overlay() {
        let mut ent = player();
        ent.armor = [item(301), None, None, None];
        assert_eq!(
            slots(&ent),
            vec![
                TextureSlot::LeatherArmor {
                    leggings: false,
                    color: DEFAULT_LEATHER
                },
                TextureSlot::LeatherOverlay { leggings: false },
            ]
        );
    }

    #[test]
    fn an_item_that_is_not_armour_is_not_drawn() {
        let mut ent = player();
        ent.armor = [None, None, None, item(276)];
        assert!(slots(&ent).is_empty());
    }

    #[test]
    fn mobs_without_a_biped_armour_layer_wear_nothing() {
        let mut ent = EntityRenderSnapshot::new(EntityId(2), EntityKind::Cow, glam::Vec3::ZERO);
        ent.armor = [None, None, None, item(306)];
        assert!(slots(&ent).is_empty());
    }

    #[test]
    fn the_dye_multiplies_the_colour_channels() {
        let mut pixels = [255, 128, 0, 200];
        tint(&mut pixels, 0x80_FF_00);
        assert_eq!(pixels, [128, 128, 0, 200], "alpha is untouched");
    }

    #[test]
    fn the_paths_follow_the_vanilla_pattern() {
        assert_eq!(
            path("leather", true, "_overlay"),
            "assets/minecraft/textures/models/armor/leather_layer_2_overlay.png"
        );
    }
}
