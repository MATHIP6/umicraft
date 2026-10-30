// SPDX-License-Identifier: MIT OR Apache-2.0

//! Item meshes placed in the world: dropped items, item frames and the items
//! in the hands of players and mobs (P3).
//!
//! Port of MCP `RenderEntityItem`: an item hovers (`sin(age / 10 + hover) *
//! 0.1 + 0.1`), spins (`age / 20 + hover` radians) and is drawn once, or as
//! two to five copies for stacks above 1, 16, 32 and 48. Lit blocks get a
//! small random offset per extra copy, flat sprites are stacked along z.
//! Items are not shaded in the world, so the mesh carries no face shade.
//!
//! The `hover` phase is random per entity in the reference; here it comes
//! from the entity id so a frame is reproducible (docs/parity.md).

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_game::hand::{bow_model_name, HandPose};
use umicraft_game::item_use::UseAction;
use umicraft_game::items;
use umicraft_mesh::item::{ItemMesh, ItemStyle};
use umicraft_mesh::VertexLight;
use umicraft_model::display::DisplayContext;
use umicraft_model::item::pose::{context_pose, ground_pose, held_pose};
use umicraft_model::item::{ItemGeometry, ItemModel};
use umicraft_model::ModelQuad;
use umicraft_world::LightLevel;

use crate::item_icons::ItemResources;
use crate::java_random::JavaRandom;

mod carried;
mod head;
mod held;
mod objects;

/// Hover height offset per unit of the ground scale (`RenderEntityItem`).
const HOVER_BASE: f32 = 0.25;
/// Offset between stacked copies of a flat sprite along z.
const SPRITE_STACK_STEP: f32 = 0.046_875;
/// Largest random offset of an extra copy of a lit block.
const BLOCK_SCATTER: f32 = 0.15;
/// The fixed seed `RenderEntityItem` sets before every item.
const SCATTER_SEED: u64 = 187;

/// The item geometry of one frame's dropped items.
#[derive(Debug, Default)]
pub struct WorldItems {
    layers: ItemLayers,
    /// Where the camera looks, for the sprites that face it.
    view: objects::View,
}

impl WorldItems {
    /// Sets the camera angles (`yaw` and `pitch` in radians, as in
    /// `umicraft_render::Camera`) that thrown items and fireballs face.
    pub fn set_view(&mut self, yaw: f32, pitch: f32) {
        self.view = objects::View::of_camera(yaw, pitch);
    }

    /// Rebuilds the meshes from the entity snapshots and returns them.
    ///
    /// The buffers of the previous frame are reused. Entities that are not
    /// dropped items, items in a frame or players holding an item, or whose
    /// item has no sprite model, add nothing.
    pub fn build(
        &mut self,
        snapshots: &[EntityRenderSnapshot],
        items: &ItemResources,
    ) -> &ItemLayers {
        self.layers.clear();
        for entity in snapshots {
            match (entity.kind, entity.item.as_ref(), entity.held.as_ref()) {
                (EntityKind::Item, Some(item), _) => {
                    push_dropped_item(&mut self.layers, entity, item, items);
                }
                (EntityKind::ItemFrame, framed, _) => {
                    push_item_frame(&mut self.layers, entity, framed, items);
                }
                (_, _, Some(held)) => held::push(&mut self.layers, entity, held, items),
                _ => {}
            }
            head::push(&mut self.layers, entity, items);
            carried::push(&mut self.layers, entity, items);
            objects::push(&mut self.layers, entity, items, self.view);
        }
        &self.layers
    }
}

/// The item geometry of a frame, and the part of it that shows the
/// enchantment glint (drawn a second time with the glint texture).
#[derive(Debug, Default)]
pub struct ItemLayers {
    /// Every item.
    pub items: ItemMesh,
    /// The enchanted items only, same geometry.
    pub glint: ItemMesh,
}

impl ItemLayers {
    /// Empties both meshes and keeps their allocations.
    pub fn clear(&mut self) {
        self.items.clear();
        self.glint.clear();
    }

    /// Appends `quads` placed by `pose`, also to the glint layer when the
    /// item is enchanted.
    fn push(&mut self, quads: &[ModelQuad], pose: &Mat4, style: &ItemStyle<'_>, glint: bool) {
        self.items.push_quads(quads, pose, style);
        if glint {
            self.glint.push_quads(quads, pose, style);
        }
    }
}

fn push_dropped_item(
    layers: &mut ItemLayers,
    entity: &EntityRenderSnapshot,
    item: &EntityItem,
    resources: &ItemResources,
) {
    let Some((model, quads)) = sprite_model(resources, item.item_id, item.damage) else {
        return;
    };
    let tints = resources.tints(item.item_id, item.damage, item.nbt.as_deref());
    let style = ItemStyle {
        atlas: resources.atlas(),
        // Items are not lit by face in the world.
        light: world_light(entity.light, false),
        tints: &tints,
    };
    let glint = resources.glints(item.item_id, item.damage, item.nbt.as_deref());
    for pose in copy_poses(entity, item.count, model) {
        layers.push(quads, &pose, &style, glint);
    }
}

/// The vertex light of an item in the world: the `light` of the block it is in,
/// without ambient occlusion, and with the face shade when `shaded`.
fn world_light(light: LightLevel, shaded: bool) -> VertexLight {
    VertexLight::new(light.block, light.sky, 15, shaded)
}

/// The bow item id.
const BOW: i16 = 261;
/// The item id of a filled map, which has its own frame model.
const FILLED_MAP: i16 = 358;

/// Appends an item frame and the item inside it (MCP `RenderItemFrame`).
///
/// The entity position of a frame is its block; the frame is drawn around the
/// block centre, turned by `180 - yaw` (the yaw is 90 per horizontal
/// direction), with its item pushed out 7/16 of a block, rolled by 45 per
/// rotation step, halved and posed for the `fixed` context. Flat sprites are
/// turned to face outwards. Maps are not drawn yet (P7).
fn push_item_frame(
    layers: &mut ItemLayers,
    entity: &EntityRenderSnapshot,
    framed: Option<&EntityItem>,
    resources: &ItemResources,
) {
    let holds_map = framed.is_some_and(|item| item.item_id == FILLED_MAP);
    let yaw = f32::from(entity.frame_direction) * 90.0;
    let base = Mat4::from_translation(entity.position.floor() + Vec3::splat(0.5))
        * Mat4::from_rotation_y((180.0 - yaw).to_radians());
    let wood = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, true),
        tints: &[],
    };
    layers.items.push_quads(
        resources.models().frame_quads(holds_map),
        &(base * Mat4::from_translation(Vec3::splat(-0.5))),
        &wood,
    );
    let Some(item) = framed.filter(|_| !holds_map) else {
        return;
    };
    let Some((model, quads)) = sprite_model(resources, item.item_id, item.damage) else {
        return;
    };
    let turn = if model.gui_3d {
        Mat4::IDENTITY
    } else {
        Mat4::from_rotation_y(std::f32::consts::PI)
    };
    let pose = base
        * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.4375))
        * Mat4::from_rotation_z(f32::from(entity.frame_rotation) * std::f32::consts::FRAC_PI_4)
        * Mat4::from_scale(Vec3::splat(0.5))
        * turn
        * context_pose(model, DisplayContext::Fixed);
    let tints = resources.tints(item.item_id, item.damage, item.nbt.as_deref());
    let style = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, model.gui_3d),
        tints: &tints,
    };
    let glint = resources.glints(item.item_id, item.damage, item.nbt.as_deref());
    layers.push(quads, &pose, &style, glint);
}

/// Appends the first-person held item to `layers`, in camera space.
///
/// `(bob, light)` is the view-bobbing transform of the frame, applied to the
/// hand as well (MCP `EntityRenderer.renderHand`), and the light at the eye. Items without a sprite model
/// (a chest, a banner) are not drawn yet.
pub fn push_held_item(
    layers: &mut ItemLayers,
    (item_id, damage, nbt): (i16, i16, Option<&[u8]>),
    hand: &HandPose,
    (bob, light): (Mat4, LightLevel),
    resources: &ItemResources,
) {
    let Some((model, quads)) = held_model(resources, item_id, damage, hand) else {
        return;
    };
    let tints = resources.tints(item_id, damage, nbt);
    let style = ItemStyle {
        atlas: resources.atlas(),
        // Lit by the world at the eye; lit blocks keep their face shade.
        light: world_light(light, model.gui_3d),
        tints: &tints,
    };
    let pose = bob * hand.item_matrix() * held_pose(model, DisplayContext::FirstPerson);
    layers.push(quads, &pose, &style, resources.glints(item_id, damage, nbt));
}

/// The model of the item in the first-person hand: the bow shows its
/// drawing model while it is drawn (MCP `RenderItem.renderItemModelForEntity`).
fn held_model<'a>(
    resources: &'a ItemResources,
    item_id: i16,
    damage: i16,
    hand: &HandPose,
) -> Option<(&'a ItemModel, &'a [ModelQuad])> {
    let drawn = hand
        .item_use
        .filter(|view| view.action == UseAction::Bow && item_id == BOW)
        .and_then(|view| bow_model_name(view.ticks_used))
        .and_then(|name| resources.models().model_by_name(name));
    match drawn {
        Some(model) => quads_of(model),
        None => sprite_model(resources, item_id, damage),
    }
}

/// The model and quads of an item that is drawn from a sprite or blocks.
fn sprite_model(
    resources: &ItemResources,
    item_id: i16,
    damage: i16,
) -> Option<(&ItemModel, &[ModelQuad])> {
    let damageable = items::lookup(i32::from(item_id)).is_some_and(items::ItemDef::is_damageable);
    let model = resources
        .models()
        .model(item_id as u16, damage as u16, damageable)?;
    quads_of(model)
}

fn quads_of(model: &ItemModel) -> Option<(&ItemModel, &[ModelQuad])> {
    match &model.geometry {
        ItemGeometry::Quads(quads) => Some((model, quads)),
        ItemGeometry::Entity => None,
    }
}

/// The world matrix of every copy of a stack.
fn copy_poses(entity: &EntityRenderSnapshot, count: u8, model: &ItemModel) -> Vec<Mat4> {
    let hover = hover_phase(entity.id.0);
    let ground = model.transform(DisplayContext::Ground);
    let bob = ((entity.age / 10.0 + hover).sin() * 0.1) + 0.1;
    let spin = (entity.age / 20.0 + hover).to_degrees();
    let copies = copy_count(count);
    let mut frame = Mat4::from_translation(
        entity.position + Vec3::new(0.0, bob + HOVER_BASE * ground.scale[1], 0.0),
    ) * Mat4::from_rotation_y(spin.to_radians());
    if !model.gui_3d {
        let back = -SPRITE_STACK_STEP * (copies - 1) as f32 * 0.5;
        frame *= Mat4::from_translation(Vec3::new(0.0, 0.0, back));
    }
    let base = ground_pose(model);
    let mut random = JavaRandom::new(SCATTER_SEED);
    let mut poses = Vec::with_capacity(copies);
    for copy in 0..copies {
        if model.gui_3d {
            let scatter = if copy > 0 {
                Vec3::new(
                    random.next_signed_float(),
                    random.next_signed_float(),
                    random.next_signed_float(),
                ) * BLOCK_SCATTER
            } else {
                Vec3::ZERO
            };
            poses.push(frame * Mat4::from_translation(scatter) * base);
        } else {
            poses.push(frame * base);
            frame *=
                Mat4::from_translation(Vec3::new(0.0, 0.0, SPRITE_STACK_STEP * ground.scale[2]));
        }
    }
    poses
}

/// How many copies a stack is drawn as (`RenderEntityItem.func_177078_a`).
fn copy_count(count: u8) -> usize {
    match count {
        49.. => 5,
        33..=48 => 4,
        17..=32 => 3,
        2..=16 => 2,
        _ => 1,
    }
}

/// A stable pseudo-random phase in `0..2π` for the hover and spin of an
/// entity (the reference draws `rand * π * 2` once per entity).
fn hover_phase(entity_id: i32) -> f32 {
    let hash = (entity_id as u32).wrapping_mul(0x9E37_79B9);
    (hash >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_enchanted_items_reach_the_glint_layer() {
        let atlas = umicraft_model::AtlasLayout::from_keys(["items/apple"]);
        let style = ItemStyle {
            atlas: &atlas,
            light: VertexLight::new(0, 15, 15, false),
            tints: &[],
        };
        let quad = ModelQuad {
            positions: [[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            normal: [0.0, 0.0, 1.0],
            uvs: [[0.0; 2]; 4],
            texture: "items/apple".to_string(),
            cullface: None,
            shade: false,
            tint: None,
        };
        let mut layers = ItemLayers::default();
        layers.push(std::slice::from_ref(&quad), &Mat4::IDENTITY, &style, false);
        assert_eq!(
            (layers.items.vertices.len(), layers.glint.vertices.len()),
            (4, 0)
        );
        layers.push(&[quad], &Mat4::IDENTITY, &style, true);
        assert_eq!(
            (layers.items.vertices.len(), layers.glint.vertices.len()),
            (8, 4)
        );
        layers.clear();
        assert!(layers.items.is_empty() && layers.glint.is_empty());
    }

    #[test]
    fn items_take_the_light_of_their_block_and_no_ambient_occlusion() {
        let lit = world_light(LightLevel::new(7, 3), true);
        assert_eq!(lit, VertexLight::new(7, 3, 15, true));
        assert_eq!(
            world_light(LightLevel::DARK, false),
            VertexLight::new(0, 0, 15, false)
        );
    }

    #[test]
    fn stacks_grow_the_copy_count_at_the_reference_thresholds() {
        let counts: Vec<usize> = [1, 2, 16, 17, 32, 33, 48, 49, 64]
            .into_iter()
            .map(copy_count)
            .collect();
        assert_eq!(counts, [1, 2, 2, 3, 3, 4, 4, 5, 5]);
    }

    #[test]
    fn the_java_generator_follows_the_documented_recurrence() {
        // The values of `new Random(187)` from the recurrence of the
        // `java.util.Random` specification (seed ^ 0x5DEECE66D, then
        // seed * 0x5DEECE66D + 0xB, top 24 bits).
        let mut random = JavaRandom::new(187);
        let values = [
            random.next_float(),
            random.next_float(),
            random.next_float(),
        ];
        let expected = [0.740_373_8_f32, 0.583_058_1, 0.438_374_76];
        for (value, expected) in values.into_iter().zip(expected) {
            assert!((value - expected).abs() < 1e-6, "{value} vs {expected}");
        }
    }

    #[test]
    fn hover_phases_are_stable_and_inside_a_turn() {
        for id in [-5, 0, 1, 77, i32::MAX] {
            let phase = hover_phase(id);
            assert!((0.0..std::f32::consts::TAU).contains(&phase), "{phase}");
            assert_eq!(phase, hover_phase(id));
        }
        assert_ne!(hover_phase(1), hover_phase(2));
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::model_resources::ModelResources;
    use umicraft_game::entity::{EntityFlags, EntityId};
    use umicraft_render::{Camera, Overlay, Renderer};

    fn dropped(id: i32, x: f32, item_id: i16, count: u8) -> EntityRenderSnapshot {
        EntityRenderSnapshot {
            id: EntityId(id),
            kind: EntityKind::Item,
            light: LightLevel::FULL_SKY,
            position: Vec3::new(x, 79.6, -3.0),
            yaw: 0.0,
            body_yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
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
            item: Some(EntityItem {
                item_id,
                damage: 0,
                count,
                nbt: None,
            }),
            age: 3.0,
            frame_direction: 0,
            frame_rotation: 0,
            armor: [None, None, None, None],
            spectator: false,
            object_data: 0,
            shadow: Default::default(),
            use_ticks: 0,
            fishing: false,
            riding: false,
            leash: None,
            flags: EntityFlags::default(),
        }
    }

    /// Renders dropped items and a held one; set `UMICRAFT_ITEM_DUMP` to a
    /// path to keep the PNG for a visual check.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn real_items_render_in_the_world_and_in_hand() {
        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(320, 180).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let mut snapshots = vec![
            dropped(1, -1.5, 276, 1),
            dropped(2, -0.5, 1, 40),
            dropped(3, 0.5, 260, 20),
            dropped(4, 1.5, 2, 1),
        ];
        // Frames on the south face of blocks behind the dropped items.
        for (id, x, item_id, rotation) in [(10, -1, 276, 0), (11, 0, 1, 0), (12, 1, 260, 2)] {
            let mut frame = dropped(id, 0.0, item_id, 1);
            frame.kind = EntityKind::ItemFrame;
            frame.position = Vec3::new(x as f32, 79.0, -6.0);
            frame.frame_direction = 0;
            frame.frame_rotation = rotation;
            snapshots.push(frame);
        }
        let mut world = WorldItems::default();
        renderer.set_world_items(&world.build(&snapshots, &items).items);
        let mut hand = ItemLayers::default();
        push_held_item(
            &mut hand,
            (276, 0, None),
            &HandPose::default(),
            (Mat4::IDENTITY, LightLevel::FULL_SKY),
            &items,
        );
        renderer.set_hand_items(&hand.items);
        renderer
            .render(&Camera::default(), &Overlay::new(320, 180))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        let first = &image.rgba[..4];
        assert!(image.rgba.chunks_exact(4).filter(|p| *p != first).count() > 1500);
        if let Ok(path) = std::env::var("UMICRAFT_ITEM_DUMP") {
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

#[cfg(test)]
mod player_render_tests {
    use super::*;
    use crate::entity_render::EntityTextures;
    use crate::model_resources::ModelResources;
    use umicraft_game::entity::{EntityFlags, EntityId};
    use umicraft_render::{Camera, Overlay, Renderer};

    fn player(id: i32, x: f32, held: i16, sneaking: bool) -> EntityRenderSnapshot {
        EntityRenderSnapshot {
            id: EntityId(id),
            kind: EntityKind::Player,
            light: LightLevel::FULL_SKY,
            position: Vec3::new(x, 78.4, -4.0),
            yaw: -70.0,
            body_yaw: -70.0,
            pitch: 0.0,
            head_yaw: -70.0,
            limb_swing: 1.0,
            limb_swing_amount: 0.6,
            swing_progress: 0.0,
            fuse_flash: 0.0,
            hurt: false,
            death_time: 0.0,
            bed: None,
            meta: Default::default(),
            cape: Default::default(),
            name_tag: None,
            profile: None,
            uuid: Some([1; 16]),
            held: Some(EntityItem {
                item_id: held,
                damage: 0,
                count: 1,
                nbt: None,
            }),
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
            riding: false,
            leash: None,
            flags: EntityFlags::new(if sneaking { 0x02 } else { 0 }, 0),
        }
    }

    #[test]
    fn the_item_is_in_the_right_hand_of_the_player() {
        // A player facing south (yaw 0) has the right hand to the west (-X).
        let mut entity = player(1, 0.0, 276, false);
        entity.yaw = 0.0;
        entity.body_yaw = 0.0;
        entity.head_yaw = 0.0;
        entity.limb_swing_amount = 0.0;
        let held = entity.held.clone().expect("a held item");
        let hand = held::hand_frame(&entity, &held)
            .expect("a player has a hand")
            .transform_point3(Vec3::ZERO);
        assert!(hand.x < entity.position.x - 0.2, "{hand}");
    }

    /// Renders two players holding a sword and a torch; set
    /// `UMICRAFT_PLAYER_DUMP` to a path to keep the PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn players_hold_their_items() {
        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(320, 180).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let snapshots = [player(1, -1.0, 276, false), player(2, 1.0, 50, true)];
        EntityTextures::load(&renderer).mesh_and_upload(&snapshots, &mut renderer);
        let mut world = WorldItems::default();
        renderer.set_world_items(&world.build(&snapshots, &items).items);
        renderer
            .render(&Camera::default(), &Overlay::new(320, 180))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_PLAYER_DUMP") {
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

#[cfg(test)]
mod use_render_tests {
    use super::*;
    use crate::model_resources::ModelResources;
    use umicraft_game::item_use::ItemUseView;
    use umicraft_render::{Camera, Overlay, Renderer};

    fn render_hand(item: (i16, i16), pose: HandPose, dump: &str) {
        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(320, 180).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let mut mesh = ItemLayers::default();
        push_held_item(
            &mut mesh,
            (item.0, item.1, None),
            &pose,
            (Mat4::IDENTITY, LightLevel::FULL_SKY),
            &items,
        );
        assert!(!mesh.items.is_empty(), "the held item has geometry");
        renderer.set_hand_items(&mesh.items);
        renderer
            .render(&Camera::default(), &Overlay::new(320, 180))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(dir) = std::env::var("UMICRAFT_USE_DUMP") {
            let file = std::fs::File::create(format!("{dir}/{dump}.png")).expect("dump file");
            let mut encoder = png::Encoder::new(file, image.width, image.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut writer| writer.write_image_data(&image.rgba))
                .expect("png");
        }
    }

    /// Renders the bow drawn, an apple being eaten and a sword blocking; set
    /// `UMICRAFT_USE_DUMP` to a directory to keep the PNGs.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn items_in_use_render_in_hand() {
        let using = |action, count: f32, ticks_used| HandPose {
            item_use: Some(ItemUseView {
                action,
                count,
                ticks_used,
            }),
            ..HandPose::default()
        };
        render_hand((261, 0), using(UseAction::Bow, 71_980.0, 20), "bow");
        render_hand((260, 0), using(UseAction::Eat, 12.0, 20), "eat");
        render_hand((276, 0), using(UseAction::Block, 71_990.0, 10), "block");
    }
}

#[cfg(test)]
mod glint_render_tests {
    use super::*;
    use crate::model_resources::ModelResources;
    use std::sync::Arc;
    use umicraft_game::entity::{EntityFlags, EntityId};
    use umicraft_render::{Camera, Overlay, Renderer};

    /// `{ench: [{id: 16, lvl: 5}]}` as a server sends it.
    fn enchanted_nbt() -> Arc<[u8]> {
        let mut nbt = vec![10, 0, 0, 9, 0, 4];
        nbt.extend(b"ench");
        nbt.extend([10, 0, 0, 0, 1, 2, 0, 2]);
        nbt.extend(b"id");
        nbt.extend(16_i16.to_be_bytes());
        nbt.extend([2, 0, 3]);
        nbt.extend(b"lvl");
        nbt.extend(5_i16.to_be_bytes());
        nbt.extend([0, 0]);
        nbt.into()
    }

    fn dropped(
        id: i32,
        x: f32,
        item_id: i16,
        damage: i16,
        nbt: Option<Arc<[u8]>>,
    ) -> EntityRenderSnapshot {
        EntityRenderSnapshot {
            id: EntityId(id),
            kind: EntityKind::Item,
            light: LightLevel::FULL_SKY,
            position: Vec3::new(x, 79.6, -2.5),
            yaw: 0.0,
            body_yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
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
            item: Some(EntityItem {
                item_id,
                damage,
                count: 1,
                nbt,
            }),
            age: 3.0,
            frame_direction: 0,
            frame_rotation: 0,
            armor: [None, None, None, None],
            spectator: false,
            object_data: 0,
            shadow: Default::default(),
            use_ticks: 0,
            fishing: false,
            riding: false,
            leash: None,
            flags: EntityFlags::default(),
        }
    }

    /// Renders enchanted and plain items with the glint; set
    /// `UMICRAFT_GLINT_DUMP` to a path to keep the PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn enchanted_items_glint_in_the_world_and_in_hand() {
        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(320, 180).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let glint = resources
            .resources()
            .glint_texture()
            .expect("the jar has the glint texture");
        renderer.set_glint_texture(glint.width, glint.height, &glint.pixels);
        renderer.set_glint_time(1.3);
        let snapshots = [
            dropped(1, -1.5, 276, 0, Some(enchanted_nbt())),
            dropped(2, -0.5, 276, 0, None),
            dropped(3, 0.5, 322, 1, None),
            dropped(4, 1.5, 322, 0, None),
        ];
        let mut world = WorldItems::default();
        let layers = world.build(&snapshots, &items);
        renderer.set_world_items(&layers.items);
        renderer.set_world_glint(&layers.glint);
        let mut hand = ItemLayers::default();
        let nbt = enchanted_nbt();
        push_held_item(
            &mut hand,
            (276, 0, Some(&nbt)),
            &HandPose::default(),
            (Mat4::IDENTITY, LightLevel::FULL_SKY),
            &items,
        );
        renderer.set_hand_items(&hand.items);
        renderer.set_hand_glint(&hand.glint);
        assert!(!layers.glint.is_empty() && !hand.glint.is_empty());
        renderer
            .render(&Camera::default(), &Overlay::new(320, 180))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_GLINT_DUMP") {
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
