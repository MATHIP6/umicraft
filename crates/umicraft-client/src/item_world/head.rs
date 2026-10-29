// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blocks worn on the head (MCP `LayerCustomHead`, `LayerSnowmanHead`).
//!
//! A pumpkin on a snowman, and any block item in the helmet slot of a
//! player, zombie, skeleton or zombie pigman. Skulls are not drawn yet.

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_mesh::item::ItemStyle;
use umicraft_model::display::DisplayContext;
use umicraft_model::entity::{ArmorMaterial, ModelPart, SnowmanModel};
use umicraft_model::item::pose::held_pose;

use super::{sprite_model, world_light, ItemLayers};
use crate::entity_render::{body_matrix, posed_humanoid_mob, posed_player, x_mirror};
use crate::item_icons::ItemResources;

/// The pumpkin block item id.
const PUMPKIN: i16 = 86;
/// Largest item id of a block item (`ItemBlock`); higher ids are items.
const LAST_BLOCK_ID: i16 = 255;

/// Appends the block `entity` wears on its head, if its kind shows one.
pub(super) fn push(
    layers: &mut ItemLayers,
    entity: &EntityRenderSnapshot,
    resources: &ItemResources,
) {
    let Some((item, frame)) = worn_block(entity) else {
        return;
    };
    let Some((model, quads)) = sprite_model(resources, item.item_id, item.damage) else {
        return;
    };
    let pose = frame * held_pose(model, DisplayContext::Head);
    let tints = resources.tints(item.item_id, item.damage, item.nbt.as_deref());
    let style = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, false),
        tints: &tints,
    };
    let glint = resources.glints(item.item_id, item.damage, item.nbt.as_deref());
    layers.push(quads, &pose, &style, glint);
}

/// The block on the head of `entity` and the frame it hangs in.
fn worn_block(entity: &EntityRenderSnapshot) -> Option<(EntityItem, Mat4)> {
    if entity.kind == EntityKind::Snowman {
        return Some((snowman_pumpkin(), snowman_frame(entity)));
    }
    let helmet = entity.armor[3].as_ref()?;
    let is_block = (0..=LAST_BLOCK_ID).contains(&helmet.item_id)
        && ArmorMaterial::from_item_id(helmet.item_id).is_none();
    if !is_block {
        return None;
    }
    biped_frame(entity).map(|frame| (helmet.clone(), frame))
}

fn snowman_pumpkin() -> EntityItem {
    EntityItem {
        item_id: PUMPKIN,
        damage: 0,
        count: 1,
        nbt: None,
    }
}

/// `LayerSnowmanHead`: the pumpkin sits 0.34375 above the head origin.
fn snowman_frame(entity: &EntityRenderSnapshot) -> Mat4 {
    let mut model = SnowmanModel::new();
    model.set_angles(entity.head_yaw - entity.body_yaw, entity.pitch);
    body_matrix(entity)
        * x_mirror()
        * head_chain(&model.head)
        * Mat4::from_translation(Vec3::new(0.0, -0.343_75, 0.0))
        * Mat4::from_rotation_y(180.0_f32.to_radians())
        * Mat4::from_scale(Vec3::new(0.625, -0.625, -0.625))
}

/// `LayerCustomHead` for a block item: the frame of the head of a biped.
fn biped_frame(entity: &EntityRenderSnapshot) -> Option<Mat4> {
    let (head, scale) = match entity.kind {
        EntityKind::Player => (posed_player(entity).0.head, 1.0),
        _ => {
            let mob = posed_humanoid_mob(entity)?;
            // Skeletons, zombies, zombie villagers and pigmen only: the giant
            // has no custom head layer.
            if entity.kind == EntityKind::Giant {
                return None;
            }
            (mob.model.head, mob.scale)
        }
    };
    // Villagers and zombie villagers sit lower in the head.
    let villager = entity.meta.is_zombie_villager();
    let sneak = if entity.flags.is_sneaking() {
        y_flip() * Mat4::from_translation(Vec3::new(0.0, 0.2, 0.0)) * y_flip()
    } else {
        Mat4::IDENTITY
    };
    // A child shrinks the whole head; a villager child keeps its size.
    let child = if entity.meta.baby && !villager {
        y_flip()
            * Mat4::from_scale(Vec3::splat(0.7))
            * Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0))
            * y_flip()
    } else {
        Mat4::IDENTITY
    };
    let lift = if villager {
        Mat4::from_translation(Vec3::new(0.0, 0.1875, 0.0))
    } else {
        Mat4::IDENTITY
    };
    Some(
        body_matrix(entity)
            * Mat4::from_scale(Vec3::splat(scale))
            * x_mirror()
            * sneak
            * child
            * head_chain(&head)
            * Mat4::from_translation(Vec3::new(0.0, -0.25, 0.0))
            * Mat4::from_rotation_y(180.0_f32.to_radians())
            * Mat4::from_scale(Vec3::new(0.625, -0.625, -0.625))
            * lift,
    )
}

/// `ModelRenderer.postRender(0.0625)` of a head: its pivot and angles, in the
/// vanilla frame (the y-up models are conjugated by a y flip).
fn head_chain(head: &ModelPart) -> Mat4 {
    head.transform(Mat4::from_scale(Vec3::splat(1.0 / 16.0)))
        * Mat4::from_scale(Vec3::splat(16.0))
        * y_flip()
}

fn y_flip() -> Mat4 {
    Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn entity(kind: EntityKind) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(0.0, 64.0, 0.0))
    }

    fn block(item_id: i16) -> Option<EntityItem> {
        Some(EntityItem {
            item_id,
            damage: 0,
            count: 1,
            nbt: None,
        })
    }

    #[test]
    fn a_snowman_always_wears_a_pumpkin() {
        let (item, _) = worn_block(&entity(EntityKind::Snowman)).expect("pumpkin");
        assert_eq!(item.item_id, PUMPKIN);
    }

    #[test]
    fn a_player_wears_a_block_but_not_armour_or_a_plain_item() {
        let mut player = entity(EntityKind::Player);
        assert!(worn_block(&player).is_none());
        player.armor[3] = block(86);
        assert_eq!(worn_block(&player).map(|(item, _)| item.item_id), Some(86));
        player.armor[3] = block(310);
        assert!(worn_block(&player).is_none(), "a diamond helmet is armour");
        player.armor[3] = block(276);
        assert!(worn_block(&player).is_none(), "a sword is not a block");
    }

    #[test]
    fn mobs_without_a_head_layer_wear_nothing() {
        let mut cow = entity(EntityKind::Cow);
        cow.armor[3] = block(86);
        assert!(worn_block(&cow).is_none());
        let mut giant = entity(EntityKind::Giant);
        giant.armor[3] = block(86);
        assert!(worn_block(&giant).is_none());
    }

    #[test]
    fn the_pumpkin_sits_above_the_snowman_body() {
        let (_, frame) = worn_block(&entity(EntityKind::Snowman)).expect("pumpkin");
        let top = frame.transform_point3(Vec3::ZERO).y;
        assert!(top > 64.0 + 1.0, "{top}");
    }

    /// Renders heads with their blocks. Set `UMICRAFT_HEAD_DUMP` to a path to
    /// keep the PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn heads_wear_their_blocks() {
        use crate::entity_render::EntityTextures;
        use crate::item_world::WorldItems;
        use crate::model_resources::ModelResources;
        use umicraft_render::{Camera, Overlay, Renderer};

        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(960, 360).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let at = |id: i32, kind: EntityKind, x: f32, helmet: Option<i16>| {
            let mut ent = EntityRenderSnapshot::new(EntityId(id), kind, Vec3::new(x, 64.0, 0.0));
            ent.yaw = 180.0;
            ent.body_yaw = 180.0;
            ent.head_yaw = 180.0;
            ent.uuid = (kind == EntityKind::Player).then_some([1; 16]);
            ent.armor[3] = helmet.and_then(block);
            ent
        };
        let enderman = |id: i32, x: f32, block: u16, screaming: bool| {
            let mut ent = at(id, EntityKind::Enderman, x, None);
            ent.meta.variant = umicraft_game::entity::meta::Variant::Enderman(
                umicraft_game::entity::meta::EndermanMeta {
                    carried: block,
                    screaming,
                },
            );
            ent
        };
        let snapshots = [
            at(1, EntityKind::Snowman, -3.0, None),
            at(2, EntityKind::Player, -1.0, Some(86)),
            at(3, EntityKind::Zombie, 1.0, Some(86)),
            at(4, EntityKind::Skeleton, 3.0, Some(35)),
            enderman(5, -5.0, 2, false),
            enderman(6, 5.0, 1, true),
        ];
        EntityTextures::load(&renderer).mesh_and_upload(&snapshots, &mut renderer);
        let mut world = WorldItems::default();
        renderer.set_world_items(&world.build(&snapshots, &items).items);
        let mut camera = Camera::new(Vec3::new(0.0, 65.8, -9.0), std::f32::consts::PI, -0.05);
        camera.set_aspect(960, 360);
        renderer
            .render(&camera, &Overlay::new(960, 360))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_HEAD_DUMP") {
            let file = std::fs::File::create(path).expect("dump file");
            let mut encoder = png::Encoder::new(file, image.width, image.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut writer| writer.write_image_data(&image.rgba))
                .expect("png");
        }
    }
}
