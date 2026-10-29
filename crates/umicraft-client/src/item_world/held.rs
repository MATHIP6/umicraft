// SPDX-License-Identifier: MIT OR Apache-2.0

//! Items in the hand of a remote player or of a mob.
//!
//! Ports MCP `LayerHeldItem` (players, zombies, skeletons, zombie pigmen,
//! giants, zombie villagers) and `LayerHeldItemWitch`. Villagers have no such
//! layer in 1.8 (`RenderVillager` only adds the head layer), so a villager
//! holds nothing whatever the server sends. Other mobs carry the stack in the
//! `Entity Equipment` slot 0 but have no layer either.

use std::borrow::Cow;

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_game::hand::bow_model_name;
use umicraft_game::items::{self, ItemKind, ToolKind};
use umicraft_mesh::item::ItemStyle;
use umicraft_model::display::DisplayContext;
use umicraft_model::entity::ModelPart;
use umicraft_model::item::pose::held_pose;
use umicraft_model::item::ItemModel;
use umicraft_model::ModelQuad;

use super::{quads_of, sprite_model, world_light, ItemLayers, BOW};
use crate::entity_render::{body_matrix, posed_humanoid_mob, posed_player, x_mirror, WITCH_SCALE};
use crate::item_icons::ItemResources;

/// The fishing rod item id.
const FISHING_ROD: i16 = 346;
/// The model of a rod with a bobber out (`ItemModelMesher` name).
const FISHING_ROD_CAST: &str = "fishing_rod_cast";
/// The carrot on a stick item id, the other item that turns around in a hand.
const CARROT_ON_A_STICK: i16 = 398;
/// Stick, bone and blaze rod: the plain items `Item.setFull3D` marks.
const FULL_3D_PLAIN: [i16; 3] = [280, 352, 369];
/// The x rotation of the witch's nose while she holds an item, in radians
/// (MCP `ModelWitch.setRotationAngles`, `field_82900_g`).
const WITCH_NOSE_PITCH: f32 = -0.9;
/// Nose wobble about z, in degrees, and the per-entity speed of its phase.
const WITCH_NOSE_ROLL_DEGREES: f32 = 2.5;
const WITCH_NOSE_SPEED: f32 = 0.01;
/// The nose pivot lies 2 model units above the neck, which is 24 up in this
/// crate's y-up models (`villagerNose.setRotationPoint(0, -2, 0)`).
const WITCH_NOSE_PIVOT: [f32; 3] = [0.0, 26.0, 0.0];

/// Appends the item `held` by `entity`, if its kind shows one.
///
/// A player with a bobber out shows a rod whatever it holds, a bow in use
/// shows the drawing model (`RenderItem.renderItemModelForEntity`).
pub(super) fn push(
    layers: &mut ItemLayers,
    entity: &EntityRenderSnapshot,
    held: &EntityItem,
    resources: &ItemResources,
) {
    let stack = shown_stack(entity, held);
    let Some((model, quads)) = shown_model(resources, entity, &stack) else {
        return;
    };
    let Some(frame) = hand_frame(entity, &stack) else {
        return;
    };
    let pose = frame * held_pose(model, DisplayContext::ThirdPerson);
    let tints = resources.tints(stack.item_id, stack.damage, stack.nbt.as_deref());
    let style = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, false),
        tints: &tints,
    };
    let glint = resources.glints(stack.item_id, stack.damage, stack.nbt.as_deref());
    layers.push(quads, &pose, &style, glint);
}

/// The stack the layer draws: a player with a bobber out holds a plain rod
/// (MCP `LayerHeldItem`: `new ItemStack(Items.fishing_rod, 0)`).
fn shown_stack<'a>(entity: &EntityRenderSnapshot, held: &'a EntityItem) -> Cow<'a, EntityItem> {
    if entity.kind == EntityKind::Player && entity.fishing {
        Cow::Owned(EntityItem {
            item_id: FISHING_ROD,
            damage: 0,
            count: 1,
            nbt: None,
        })
    } else {
        Cow::Borrowed(held)
    }
}

/// The name of the model that replaces the one of the stack, for players
/// only (MCP `RenderItem.renderItemModelForEntity`).
fn special_model_name(entity: &EntityRenderSnapshot, stack: &EntityItem) -> Option<&'static str> {
    if entity.kind != EntityKind::Player {
        return None;
    }
    if entity.fishing {
        return Some(FISHING_ROD_CAST);
    }
    if stack.item_id == BOW && entity.flags.is_using_item() {
        return bow_model_name(entity.use_ticks);
    }
    None
}

fn shown_model<'a>(
    resources: &'a ItemResources,
    entity: &EntityRenderSnapshot,
    stack: &EntityItem,
) -> Option<(&'a ItemModel, &'a [ModelQuad])> {
    let named = special_model_name(entity, stack)
        .and_then(|name| resources.models().model_by_name(name))
        .and_then(quads_of);
    named.or_else(|| sprite_model(resources, stack.item_id, stack.damage))
}

/// The frame of an item in the hand of `entity`, up to the item's own pose;
/// `None` for kinds with no held-item layer.
pub(super) fn hand_frame(entity: &EntityRenderSnapshot, stack: &EntityItem) -> Option<Mat4> {
    if entity.kind == EntityKind::Witch {
        return Some(witch_frame(entity, stack.item_id));
    }
    biped_frame(entity, stack.item_id)
}

/// The right-hand frame of a biped (`LayerHeldItem`).
///
/// Vanilla places the item inside the frame of the model, which is drawn
/// through `scale(-1, -1, 1)`; the y half is the y flip of the arm chain and
/// the x half is the model mirror, applied once on the outside. The item is
/// *not* mirrored back: its display transforms are authored for that frame.
/// The arm frame of the vanilla model has y pointing down while this crate's
/// models have it pointing up, so the chain is conjugated by a y flip (the
/// arm boxes are the same boxes with negated y).
fn biped_frame(entity: &EntityRenderSnapshot, item_id: i16) -> Option<Mat4> {
    let (arm, scale) = if entity.kind == EntityKind::Player {
        (posed_player(entity).0.right_arm, 1.0)
    } else {
        let mob = posed_humanoid_mob(entity)?;
        (mob.model.right_arm, mob.scale)
    };
    let arm = arm.transform(Mat4::from_scale(Vec3::splat(1.0 / 16.0)))
        * Mat4::from_scale(Vec3::splat(16.0))
        * y_flip();
    let child = if entity.meta.baby {
        y_flip() * baby_transform() * y_flip()
    } else {
        Mat4::IDENTITY
    };
    Some(
        body_matrix(entity)
            * Mat4::from_scale(Vec3::splat(scale))
            * x_mirror()
            * child
            * arm
            * hand_offset(entity, item_id),
    )
}

/// What `LayerHeldItem` does to a child before the arm: lower, tilt and halve
/// (in the vanilla frame).
fn baby_transform() -> Mat4 {
    Mat4::from_translation(Vec3::new(0.0, 0.625, 0.0))
        * Mat4::from_rotation_x(20.0_f32.to_radians())
        * Mat4::from_scale(Vec3::splat(0.5))
}

/// From the arm to the palm: translate(-1/16, 7/16, 1/16), the pose of
/// torches and the sneak lift.
fn hand_offset(entity: &EntityRenderSnapshot, item_id: i16) -> Mat4 {
    let mut hand = Mat4::from_translation(Vec3::new(-0.0625, 0.4375, 0.0625));
    if is_torch(item_id) {
        hand *= Mat4::from_translation(Vec3::new(0.0, 0.1875, -0.3125))
            * Mat4::from_rotation_x(20.0_f32.to_radians())
            * Mat4::from_rotation_y(45.0_f32.to_radians())
            * Mat4::from_scale(Vec3::new(-0.375, -0.375, 0.375));
    }
    if entity.flags.is_sneaking() {
        hand *= Mat4::from_translation(Vec3::new(0.0, 0.203_125, 0.0));
    }
    hand
}

/// The witch holds the item in front of her nose (`LayerHeldItemWitch`).
///
/// `ModelRenderer.postRender` of the nose applies its own rotation point and
/// angles only, not those of the head it hangs on. The nose is pitched while
/// she holds something and wobbles about z with the age of the entity.
fn witch_frame(entity: &EntityRenderSnapshot, item_id: i16) -> Mat4 {
    let phase = WITCH_NOSE_SPEED * (entity.id.0.rem_euclid(10)) as f32;
    let roll = (entity.age * phase).cos() * WITCH_NOSE_ROLL_DEGREES.to_radians();
    let mut nose = ModelPart::new(WITCH_NOSE_PIVOT);
    // Vanilla angles `(x, y, z)` are `(-x, y, -z)` in the y-up models.
    nose.rotation = [-WITCH_NOSE_PITCH, 0.0, -roll];
    let nose = nose.transform(Mat4::from_scale(Vec3::splat(1.0 / 16.0)))
        * Mat4::from_scale(Vec3::splat(16.0))
        * y_flip();
    body_matrix(entity)
        * Mat4::from_scale(Vec3::splat(WITCH_SCALE))
        * x_mirror()
        * nose
        * Mat4::from_translation(Vec3::new(-0.0625, 0.53125, 0.21875))
        * witch_item_pose(item_id)
        * Mat4::from_rotation_x((-15.0_f32).to_radians())
        * Mat4::from_rotation_z(40.0_f32.to_radians())
}

/// The part of `LayerHeldItemWitch` that depends on the kind of item: a bow,
/// a handle-like item (`isFull3D`) or a flat one. Chest-like block items have
/// no sprite model and are not drawn.
fn witch_item_pose(item_id: i16) -> Mat4 {
    let rotate = |degrees: f32, axis: Vec3| Mat4::from_axis_angle(axis, degrees.to_radians());
    if item_id == BOW {
        Mat4::from_translation(Vec3::new(0.0, 0.125, -0.125))
            * rotate(-45.0, Vec3::Y)
            * Mat4::from_scale(Vec3::new(0.625, -0.625, 0.625))
            * rotate(-100.0, Vec3::X)
            * rotate(-20.0, Vec3::Y)
    } else if is_full_3d(item_id) {
        let turned = if turns_around(item_id) {
            rotate(180.0, Vec3::Z) * Mat4::from_translation(Vec3::new(0.0, -0.0625, 0.0))
        } else {
            Mat4::IDENTITY
        };
        // `RenderWitch.transformHeldFull3DItemLayer`, then the layer's own.
        turned
            * Mat4::from_translation(Vec3::new(0.0, 0.1875, 0.0))
            * Mat4::from_translation(Vec3::new(0.0625, -0.125, 0.0))
            * Mat4::from_scale(Vec3::new(0.625, -0.625, 0.625))
    } else {
        Mat4::from_translation(Vec3::new(0.1875, 0.1875, 0.0))
            * Mat4::from_scale(Vec3::splat(0.875))
            * rotate(-20.0, Vec3::Z)
            * rotate(-60.0, Vec3::X)
            * rotate(-30.0, Vec3::Z)
    }
}

fn y_flip() -> Mat4 {
    Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
}

/// The blocks whose item uses the torch pose in a hand (`getRenderType() == 2`).
pub(super) fn is_torch(item_id: i16) -> bool {
    matches!(item_id, 50 | 75 | 76)
}

/// MCP `Item.isFull3D`: tools and swords (`ItemTool`, `ItemSword`,
/// `ItemHoe`, shears excluded), the fishing rod, the carrot on a stick, the
/// stick, the bone and the blaze rod.
fn is_full_3d(item_id: i16) -> bool {
    if item_id == FISHING_ROD || item_id == CARROT_ON_A_STICK || FULL_3D_PLAIN.contains(&item_id) {
        return true;
    }
    matches!(
        items::lookup(i32::from(item_id)).map(|def| def.kind),
        Some(ItemKind::Tool(kind, _)) if kind != ToolKind::Shears
    )
}

/// MCP `shouldRotateAroundWhenRendering`: only the rod and the carrot stick.
fn turns_around(item_id: i16) -> bool {
    item_id == FISHING_ROD || item_id == CARROT_ON_A_STICK
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId};

    const IRON_SWORD: i16 = 267;
    const GOLDEN_SWORD: i16 = 283;
    const SHEARS: i16 = 359;
    const POTION: i16 = 373;

    fn entity(kind: EntityKind) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(4), kind, Vec3::ZERO)
    }

    fn stack(item_id: i16) -> EntityItem {
        EntityItem {
            item_id,
            damage: 0,
            count: 1,
            nbt: None,
        }
    }

    #[test]
    fn handle_like_items_are_full_3d_and_flat_ones_are_not() {
        for id in [IRON_SWORD, GOLDEN_SWORD, 257, 292, FISHING_ROD, 280, 369] {
            assert!(is_full_3d(id), "{id}");
        }
        for id in [SHEARS, POTION, 260, 4] {
            assert!(!is_full_3d(id), "{id}");
        }
        assert!(turns_around(FISHING_ROD) && !turns_around(IRON_SWORD));
    }

    #[test]
    fn a_bow_in_use_and_a_cast_rod_pick_their_models() {
        let mut player = entity(EntityKind::Player);
        player.flags = EntityFlags::new(0x10, 0);
        player.use_ticks = 20;
        assert_eq!(
            special_model_name(&player, &stack(BOW)),
            Some("bow_pulling_2")
        );
        assert_eq!(special_model_name(&player, &stack(IRON_SWORD)), None);
        player.flags = EntityFlags::default();
        assert_eq!(special_model_name(&player, &stack(BOW)), None);
        player.fishing = true;
        assert_eq!(
            special_model_name(&player, &stack(FISHING_ROD)),
            Some(FISHING_ROD_CAST)
        );
    }

    #[test]
    fn mobs_never_use_the_player_only_models() {
        let mut skeleton = entity(EntityKind::Skeleton);
        skeleton.flags = EntityFlags::new(0x10, 0);
        skeleton.use_ticks = 20;
        assert_eq!(special_model_name(&skeleton, &stack(BOW)), None);
    }

    #[test]
    fn a_bobber_turns_whatever_a_player_holds_into_a_rod() {
        let mut player = entity(EntityKind::Player);
        let sword = stack(IRON_SWORD);
        assert_eq!(shown_stack(&player, &sword).item_id, IRON_SWORD);
        player.fishing = true;
        assert_eq!(shown_stack(&player, &sword).item_id, FISHING_ROD);
        let mut zombie = entity(EntityKind::Zombie);
        zombie.fishing = true;
        assert_eq!(shown_stack(&zombie, &sword).item_id, IRON_SWORD);
    }

    #[test]
    fn bipeds_and_the_witch_have_a_hand_but_villagers_and_cows_do_not() {
        let sword = stack(GOLDEN_SWORD);
        for kind in [
            EntityKind::Player,
            EntityKind::Zombie,
            EntityKind::Skeleton,
            EntityKind::ZombiePigman,
            EntityKind::Giant,
            EntityKind::ZombieVillager,
            EntityKind::Witch,
        ] {
            assert!(hand_frame(&entity(kind), &sword).is_some(), "{kind:?}");
        }
        for kind in [EntityKind::Villager, EntityKind::Cow, EntityKind::Creeper] {
            assert!(hand_frame(&entity(kind), &sword).is_none(), "{kind:?}");
        }
    }

    #[test]
    fn a_zombie_holds_the_item_on_its_right_and_in_front_of_it() {
        // Yaw 0 faces south (+Z): the right hand is to the west (-X) and the
        // arms are stretched forward.
        let hand = hand_frame(&entity(EntityKind::Zombie), &stack(GOLDEN_SWORD))
            .expect("zombie hand")
            .transform_point3(Vec3::ZERO);
        assert!(hand.x < -0.2, "{hand}");
        assert!(hand.z > 0.3, "{hand}");
        assert!((0.9..1.6).contains(&hand.y), "{hand}");
    }

    #[test]
    fn a_giant_holds_the_item_six_times_as_far_out() {
        let sword = stack(GOLDEN_SWORD);
        let zombie = hand_frame(&entity(EntityKind::Zombie), &sword).expect("zombie");
        let giant = hand_frame(&entity(EntityKind::Giant), &sword).expect("giant");
        let (zombie, giant) = (
            zombie.transform_point3(Vec3::ZERO),
            giant.transform_point3(Vec3::ZERO),
        );
        assert!((giant - zombie * 6.0).length() < 1e-3, "{zombie} {giant}");
    }

    #[test]
    fn a_baby_holds_the_item_lower_than_an_adult() {
        let sword = stack(GOLDEN_SWORD);
        let adult = entity(EntityKind::Zombie);
        let mut baby = adult.clone();
        baby.meta.baby = true;
        let height = |e: &EntityRenderSnapshot| {
            hand_frame(e, &sword)
                .expect("hand")
                .transform_point3(Vec3::ZERO)
                .y
        };
        assert!(height(&baby) < height(&adult));
    }

    #[test]
    fn the_witch_holds_the_item_in_front_of_her_folded_arms() {
        let hand = hand_frame(&entity(EntityKind::Witch), &stack(POTION))
            .expect("witch")
            .transform_point3(Vec3::ZERO);
        assert!(hand.z > 0.1, "in front of her (south at yaw 0): {hand}");
        assert!((0.6..1.3).contains(&hand.y), "{hand}");
    }
}
