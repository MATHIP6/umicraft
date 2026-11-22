// SPDX-License-Identifier: MIT OR Apache-2.0

//! Objects drawn with item or block models: thrown items and fireballs as
//! sprites facing the camera, falling blocks and primed TNT as blocks (MCP
//! `RenderSnowball`, `RenderFireball`, `RenderFallingBlock`,
//! `RenderTntPrimed`).

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_mesh::item::ItemStyle;
use umicraft_model::display::DisplayContext;
use umicraft_model::item::pose::context_pose;
use umicraft_model::ModelQuad;

use super::{entity_stack_model, world_light, ItemLayers};
use crate::resources::item_icons::ItemResources;
use crate::scene::entity::vehicles::wobble;

/// Where the camera looks, in the angles of the reference (`playerViewY` is
/// the yaw, `playerViewX` the pitch, positive down), in degrees.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct View {
    /// `RenderManager.playerViewY`.
    pub yaw: f32,
    /// `RenderManager.playerViewX`.
    pub pitch: f32,
}

impl View {
    /// The view of a render camera (`yaw` and `pitch` in radians, in the
    /// convention of `umicraft_render::Camera`).
    #[must_use]
    pub fn of_camera(yaw: f32, pitch: f32) -> Self {
        Self {
            yaw: (yaw - std::f32::consts::PI).to_degrees(),
            pitch: (-pitch).to_degrees(),
        }
    }
}

/// The fuse of a primed TNT when it is created, in ticks.
const TNT_FUSE: f32 = 80.0;
/// The block id of TNT.
const TNT: i16 = 46;
/// The blocks of the minecarts that carry one.
pub(crate) const CHEST: i16 = 54;
const FURNACE: i16 = 61;
/// The burning furnace, a block without an item.
const LIT_FURNACE: i16 = 62;
pub(crate) const MOB_SPAWNER: i16 = 52;
const HOPPER: i16 = 154;
const COMMAND_BLOCK: i16 = 137;
/// The `data` of the spawn packet of the carts whose renderer does more
/// than draw the block (`EntityMinecart.EnumMinecartType`).
const FURNACE_CART: i32 = 2;
const TNT_CART: i32 = 3;
pub(crate) const SPAWNER_CART: i32 = 4;
/// The fire charge item, the sprite of fireballs.
const FIRE_CHARGE: i16 = 385;

/// How an object is placed.
pub(crate) enum Pose {
    /// A sprite facing the camera: the frame, then the pose of the item for
    /// its ground context.
    Sprite(Mat4),
    /// The quads of the model (`0..1`) under a fixed matrix.
    Block(Mat4),
}

/// Appends the object `entity` if it is drawn with an item or block model.
pub(super) fn push(
    layers: &mut ItemLayers,
    entity: &EntityRenderSnapshot,
    resources: &ItemResources,
    view: View,
) {
    let Some((item, pose)) = placed(entity, view) else {
        return;
    };
    let (quads, matrix) = match pose {
        // The burning furnace of a cart has no item: its block model.
        Pose::Block(matrix) if item.item_id == LIT_FURNACE => {
            (resources.models().lit_furnace_quads(), matrix)
        }
        Pose::Block(matrix) => {
            let Some(quads) = block_quads(resources, &item) else {
                return;
            };
            (quads, matrix)
        }
        Pose::Sprite(frame) => {
            let Some((model, quads)) = entity_stack_model(resources, &item) else {
                return;
            };
            (quads, frame * context_pose(model, DisplayContext::Ground))
        }
    };
    let tints = resources.tints(item.item_id, item.damage, None);
    let lit = takes_item_lights(entity.kind);
    let style = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, !lit),
        tints: &tints,
    };
    if lit {
        layers.push_world_lit(quads, &matrix, &style, false);
    } else {
        layers.push(quads, &matrix, &style, false);
    }
}

/// The quads of a block drawn whole: by its id and metadata, else by its id
/// alone, since the item of a block only has models for the metadata that
/// change its look (a cart's tile set by a command may face any way).
fn block_quads<'a>(resources: &'a ItemResources, item: &EntityItem) -> Option<&'a [ModelQuad]> {
    entity_stack_model(resources, item)
        .or_else(|| {
            let plain = EntityItem {
                damage: 0,
                ..item.clone()
            };
            entity_stack_model(resources, &plain)
        })
        .map(|(_, quads)| quads)
}

/// Whether an object of `kind` is lit by the item lights of the entity pass
/// rather than by the face shade of the world.
///
/// `RenderFallingBlock` turns the lights off and keeps the face shade baked
/// in the block quads; the sprites (`RenderItem`), the primed TNT and the
/// blocks of the carts (`renderBlockBrightness`) replace that colour, so the
/// lights alone shade them.
fn takes_item_lights(kind: EntityKind) -> bool {
    kind != EntityKind::FallingBlock
}

/// The item an object shows and the pose that places it.
fn placed(entity: &EntityRenderSnapshot, view: View) -> Option<(EntityItem, Pose)> {
    let at = Mat4::from_translation(entity.position);
    let item = |item_id: i16, damage: i16| EntityItem {
        item_id,
        damage,
        count: 1,
        nbt: None,
    };
    match entity.kind {
        EntityKind::Snowball => Some((item(332, 0), thrown(at, view))),
        EntityKind::Egg => Some((item(344, 0), thrown(at, view))),
        EntityKind::EnderPearl => Some((item(368, 0), thrown(at, view))),
        EntityKind::EyeOfEnder => Some((item(381, 0), thrown(at, view))),
        EntityKind::ExpBottle => Some((item(384, 0), thrown(at, view))),
        EntityKind::FireworkRocket => Some((item(401, 0), thrown(at, view))),
        EntityKind::Potion => Some((item(373, entity.object_data as i16), thrown(at, view))),
        EntityKind::LargeFireball => Some((item(FIRE_CHARGE, 0), fireball(at, 2.0, view))),
        EntityKind::SmallFireball => Some((item(FIRE_CHARGE, 0), fireball(at, 0.5, view))),
        EntityKind::FallingBlock => Some(falling_block(entity)),
        EntityKind::PrimedTnt => Some((item(TNT, 0), tnt(entity))),
        EntityKind::Minecart => cart_block(entity),
        _ => None,
    }
}

/// The block a minecart carries by its type, the `data` of its spawn packet
/// (`getDefaultDisplayTile` of each cart class), and how many pixels it sits
/// over the floor (`getDefaultDisplayTileOffset`). A furnace cart that burns
/// fuel shows the lit furnace.
fn default_tile(entity: &EntityRenderSnapshot) -> Option<(i16, i32)> {
    Some(match entity.object_data {
        1 => (CHEST, 8),
        FURNACE_CART if entity.meta.cart().is_some_and(|cart| cart.powered) => (LIT_FURNACE, 6),
        FURNACE_CART => (FURNACE, 6),
        TNT_CART => (TNT, 6),
        SPAWNER_CART => (MOB_SPAWNER, 6),
        5 => (HOPPER, 1),
        6 => (COMMAND_BLOCK, 6),
        _ => return None,
    })
}

/// The block a cart shows and its height over the floor in pixels: the tile
/// a command set (entries 20 to 22) or the default one of its type
/// (`getDisplayTile`, `getDisplayTileOffset`). Air shows nothing.
fn display_tile(entity: &EntityRenderSnapshot) -> Option<(EntityItem, i32)> {
    let block = |item_id: i16, damage: i16| EntityItem {
        item_id,
        damage,
        count: 1,
        nbt: None,
    };
    match entity.meta.cart().and_then(|cart| cart.display_tile()) {
        Some(tile) if tile.block_id() == 0 => None,
        Some(tile) => Some((
            block(tile.block_id() as i16, tile.metadata() as i16),
            tile.offset,
        )),
        None => default_tile(entity).map(|(id, offset)| (block(id, 0), offset)),
    }
}

/// The frame a cart draws its block in (`RenderMinecart.doRender`): the
/// cart's own frame (the model is drawn 0.375 up, turned to its yaw, tipped
/// by its pitch and rolled by its shake), then three quarters of the size and
/// the block's corner `offset` pixels over the floor.
pub(crate) fn cart_frame(entity: &EntityRenderSnapshot, offset: i32) -> Mat4 {
    Mat4::from_translation(entity.position + cart_jitter(entity.id.0) + Vec3::new(0.0, 0.375, 0.0))
        * Mat4::from_rotation_y((180.0 - entity.yaw).to_radians())
        * Mat4::from_rotation_z((-entity.pitch).to_radians())
        * Mat4::from_rotation_x(wobble(entity).to_radians())
        * Mat4::from_scale(Vec3::splat(0.75))
        * Mat4::from_translation(Vec3::new(-0.5, (offset - 8) as f32 / 16.0, 0.5))
}

/// The few thousandths of a block every cart is moved by its id, so that
/// carts on the same spot do not flicker into each other
/// (`RenderMinecart.doRender`, the hash of `getEntityId`).
pub(crate) fn cart_jitter(id: i32) -> Vec3 {
    let mut hash = i64::from(id).wrapping_mul(493_286_711);
    hash = hash
        .wrapping_mul(hash)
        .wrapping_mul(4_392_167_121)
        .wrapping_add(hash.wrapping_mul(98_761));
    let axis = |shift: u32| ((((hash >> shift) & 7) as f32 + 0.5) / 8.0 - 0.5) * 0.004;
    Vec3::new(axis(16), axis(20), axis(24))
}

/// `minecartTNTFuse - partialTicks + 1` of a lit TNT cart, `None` while it
/// is not lit.
fn cart_fuse_remaining(entity: &EntityRenderSnapshot) -> Option<(i32, f32)> {
    if entity.object_data != TNT_CART {
        return None;
    }
    let fuse = entity.meta.cart()?.fuse;
    (fuse > -1).then(|| (fuse, fuse as f32 - entity.age.fract() + 1.0))
}

/// The block a minecart carries, placed in the cart; a lit TNT cart swells
/// its block in the last 10 ticks (`RenderTntMinecart.func_180560_a`).
pub(crate) fn cart_block(entity: &EntityRenderSnapshot) -> Option<(EntityItem, Pose)> {
    let (item, offset) = display_tile(entity)?;
    let grown = cart_fuse_remaining(entity).map_or(1.0, |(_, remaining)| swell(remaining));
    // The quarter turn is `renderBlockBrightness`'s.
    let matrix = cart_frame(entity, offset)
        * Mat4::from_scale(Vec3::splat(grown))
        * Mat4::from_rotation_y(90.0_f32.to_radians());
    Some((item, Pose::Block(matrix)))
}

/// Where the white flash of a lit TNT cart goes
/// (`RenderTntMinecart.func_180560_a`: every second 5 ticks of the fuse, a
/// unit TNT cube in the swollen frame of the block), `None` between flashes.
pub(crate) fn cart_flash(entity: &EntityRenderSnapshot) -> Option<Mat4> {
    let (fuse, remaining) = cart_fuse_remaining(entity)?;
    if !flash_on(fuse) {
        return None;
    }
    let offset = display_tile(entity).map_or(6, |(_, offset)| offset);
    // `renderBlockBrightness` turns the white TNT a quarter, as the block.
    let matrix = cart_frame(entity, offset)
        * Mat4::from_scale(Vec3::splat(swell(remaining)))
        * Mat4::from_rotation_y(90.0_f32.to_radians());
    Some(matrix)
}

/// The frame of the spinning mob of a spawner cart that shows its spawner
/// (`RenderMinecartMobSpawner`).
pub(crate) fn cart_spawner_frame(entity: &EntityRenderSnapshot) -> Option<Mat4> {
    if entity.object_data != SPAWNER_CART {
        return None;
    }
    let (item, offset) = display_tile(entity)?;
    (item.item_id == MOB_SPAWNER).then(|| cart_frame(entity, offset))
}

/// How much a TNT block has grown with `remaining` ticks of fuse
/// (`fuse - partialTicks + 1`): from 1 to 1.3 over the last 10.
fn swell(remaining: f32) -> f32 {
    if remaining < 10.0 {
        let grown = (1.0 - remaining / 10.0).clamp(0.0, 1.0);
        1.0 + grown.powi(4) * 0.3
    } else {
        1.0
    }
}

/// Whether the white flash of TNT shows with `fuse` whole ticks left: every
/// second 5 ticks (`fuse / 5 % 2 == 0`).
fn flash_on(fuse: i32) -> bool {
    fuse >= 0 && (fuse / 5) % 2 == 0
}

/// `RenderSnowball.doRender`: half size, turned to face the camera.
fn thrown(at: Mat4, view: View) -> Pose {
    Pose::Sprite(
        at * Mat4::from_scale(Vec3::splat(0.5))
            * Mat4::from_rotation_y((-view.yaw).to_radians())
            * Mat4::from_rotation_x(view.pitch.to_radians()),
    )
}

/// `RenderFireball.doRender`: the fire charge as a sprite `scale` times
/// larger, facing the camera, standing a quarter of a block above its centre.
fn fireball(at: Mat4, scale: f32, view: View) -> Pose {
    Pose::Block(
        at * Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_rotation_y((180.0 - view.yaw).to_radians())
            * Mat4::from_rotation_x((-view.pitch).to_radians())
            * Mat4::from_translation(Vec3::new(-0.5, -0.25, -0.5)),
    )
}

/// `RenderFallingBlock.doRender`: the block of the spawn packet (`id` in the
/// low 12 bits, the data above), centred on the entity with its bottom at the
/// entity height.
fn falling_block(entity: &EntityRenderSnapshot) -> (EntityItem, Pose) {
    let state = entity.object_data & 0xFFFF;
    let item = EntityItem {
        item_id: (state & 0x0FFF) as i16,
        damage: (state >> 12) as i16,
        count: 1,
        nbt: None,
    };
    let matrix = Mat4::from_translation(entity.position + Vec3::new(-0.5, 0.0, -0.5));
    (item, Pose::Block(matrix))
}

/// `RenderTntPrimed.doRender`: the block of TNT, growing in the last 10 ticks
/// of the fuse. Only the lit block is drawn here, not the white flash.
fn tnt(entity: &EntityRenderSnapshot) -> Pose {
    // `translate(-0.5, -0.5, 0.5)` then the quarter turn of
    // `renderBlockBrightness`.
    Pose::Block(
        Mat4::from_translation(entity.position + Vec3::new(0.0, 0.5, 0.0))
            * Mat4::from_scale(Vec3::splat(swell(fuse_remaining(entity.age))))
            * Mat4::from_translation(Vec3::new(-0.5, -0.5, 0.5))
            * Mat4::from_rotation_y(90.0_f32.to_radians()),
    )
}

/// Where the block of a primed TNT is, for the white flash drawn over it.
pub(crate) fn tnt_matrix(entity: &EntityRenderSnapshot) -> Mat4 {
    match tnt(entity) {
        Pose::Block(matrix) | Pose::Sprite(matrix) => matrix,
    }
}

/// `fuse - partialTicks + 1` for an entity of `age` ticks: the fuse starts at
/// 80 and goes down by one per tick.
fn fuse_remaining(age: f32) -> f32 {
    TNT_FUSE - age + 1.0
}

/// Where the flash of a primed TNT of `age` ticks goes
/// (`RenderTntPrimed.doRender`), `None` between flashes.
pub(crate) fn primed_flash(entity: &EntityRenderSnapshot) -> Option<Mat4> {
    let fuse = (TNT_FUSE - entity.age.floor()) as i32;
    flash_on(fuse).then(|| tnt_matrix(entity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn entity(kind: EntityKind) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(10.0, 70.0, 4.0))
    }

    fn matrix(pose: &Pose) -> Mat4 {
        match pose {
            Pose::Block(matrix) | Pose::Sprite(matrix) => *matrix,
        }
    }

    #[test]
    fn the_view_of_a_camera_is_the_vanilla_yaw_and_pitch() {
        // A camera that looks south (+Z) has yaw 180 degrees in this crate.
        let south = View::of_camera(std::f32::consts::PI, 0.0);
        assert!(south.yaw.abs() < 1e-4);
        // Looking down by 30 degrees is a positive pitch.
        assert!((View::of_camera(0.0, -30.0_f32.to_radians()).pitch - 30.0).abs() < 1e-3);
    }

    #[test]
    fn a_thrown_sprite_turns_with_the_camera_and_is_half_size() {
        let at = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let front = matrix(&thrown(at, View::default()));
        let side = matrix(&thrown(
            at,
            View {
                yaw: 90.0,
                pitch: 0.0,
            },
        ));
        assert_ne!(front, side);
        assert!((front.transform_vector3(Vec3::X).length() - 0.5).abs() < 1e-5);
        assert_eq!(front.transform_point3(Vec3::ZERO), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn a_fireball_is_scaled_and_centred_a_quarter_above() {
        let at = Mat4::IDENTITY;
        let small = matrix(&fireball(at, 0.5, View::default()));
        let large = matrix(&fireball(at, 2.0, View::default()));
        // The middle of the sprite square (0.5, 0.5, 0.5) is `0.25 * scale` up.
        let centre = |m: Mat4| m.transform_point3(Vec3::splat(0.5));
        assert!((centre(small).y - 0.25 * 0.5).abs() < 1e-5);
        assert!((centre(large).y - 0.25 * 2.0).abs() < 1e-5);
        assert!(centre(large).x.abs() < 1e-5 && centre(large).z.abs() < 1e-5);
    }

    #[test]
    fn tnt_is_a_block_centred_on_the_entity_and_swells_before_it_blows() {
        let mut lit = entity(EntityKind::PrimedTnt);
        lit.age = 5.0;
        let calm = matrix(&tnt(&lit));
        // The centre of the block is half a block above the entity.
        let centre = calm.transform_point3(Vec3::splat(0.5));
        assert!(
            (centre - Vec3::new(10.0, 70.5, 4.0)).length() < 1e-4,
            "{centre}"
        );
        assert!((calm.transform_vector3(Vec3::X).length() - 1.0).abs() < 1e-5);
        // 3 ticks from the end of the 80 tick fuse: it has grown.
        lit.age = 78.0;
        let swollen = matrix(&tnt(&lit));
        let size = swollen.transform_vector3(Vec3::X).length();
        assert!(size > 1.0 && size <= 1.3 + 1e-5, "{size}");
    }

    #[test]
    fn a_cart_carries_the_block_of_its_type_inside_itself() {
        let mut cart = entity(EntityKind::Minecart);
        // An empty cart (0) and an unknown type carry nothing.
        assert!(cart_block(&cart).is_none());
        cart.object_data = 9;
        assert!(cart_block(&cart).is_none());
        // A TNT cart: the block at three quarters of its size, 0.375 over the
        // position plus (6 - 8) / 16 of the scaled block.
        cart.object_data = 3;
        let (item, pose) = cart_block(&cart).expect("a block");
        assert_eq!(item.item_id, TNT);
        let centre = matrix(&pose).transform_point3(Vec3::splat(0.5));
        let expected_y = 70.0 + 0.375 + 0.75 * (-2.0 / 16.0 + 0.5);
        // Within the 0.002 the id moves a cart by.
        assert!(
            (centre.y - expected_y).abs() < 0.0021,
            "{} {expected_y}",
            centre.y
        );
        assert!((matrix(&pose).transform_vector3(Vec3::X).length() - 0.75).abs() < 1e-5);
        // The chest sits two pixels higher than the furnace.
        cart.object_data = 1;
        let chest = matrix(&cart_block(&cart).unwrap().1).transform_point3(Vec3::ZERO);
        cart.object_data = 2;
        let furnace = matrix(&cart_block(&cart).unwrap().1).transform_point3(Vec3::ZERO);
        assert!((chest.y - furnace.y - 0.75 * 2.0 / 16.0).abs() < 1e-4);
    }

    fn cart_meta(
        objects: Vec<umicraft_protocol::packets::play::metadata::WatchedObject>,
    ) -> umicraft_game::entity::EntityMeta {
        let mut meta = umicraft_game::entity::EntityMeta::default();
        meta.apply(
            EntityKind::Minecart,
            &umicraft_protocol::packets::play::Metadata { objects },
        );
        meta
    }

    fn watched(
        type_id: u8,
        index: u8,
        value: Vec<u8>,
    ) -> umicraft_protocol::packets::play::metadata::WatchedObject {
        umicraft_protocol::packets::play::metadata::WatchedObject {
            type_id,
            index,
            value,
        }
    }

    #[test]
    fn a_burning_furnace_cart_shows_the_lit_furnace() {
        let mut cart = entity(EntityKind::Minecart);
        cart.object_data = 2;
        assert_eq!(cart_block(&cart).expect("furnace").0.item_id, FURNACE);
        cart.meta = cart_meta(vec![watched(0, 16, vec![1])]);
        assert_eq!(cart_block(&cart).expect("lit").0.item_id, LIT_FURNACE);
    }

    #[test]
    fn a_custom_tile_replaces_the_block_of_the_type_and_air_hides_it() {
        let mut cart = entity(EntityKind::Minecart);
        cart.object_data = 1;
        // Red wool (35:14) two pixels over the floor.
        cart.meta = cart_meta(vec![
            watched(2, 20, (35_i32 | 14 << 12).to_be_bytes().to_vec()),
            watched(2, 21, 2_i32.to_be_bytes().to_vec()),
            watched(0, 22, vec![1]),
        ]);
        let (item, pose) = cart_block(&cart).expect("wool");
        assert_eq!((item.item_id, item.damage), (35, 14));
        let low = matrix(&pose).transform_point3(Vec3::ZERO).y;
        assert!(
            (low - (70.0 + 0.375 + 0.75 * (2.0 - 8.0) / 16.0)).abs() < 0.0021,
            "{low}"
        );
        // Even an empty cart shows a custom tile; air shows nothing.
        cart.object_data = 0;
        assert!(cart_block(&cart).is_some());
        cart.meta = cart_meta(vec![watched(2, 20, vec![0; 4]), watched(0, 22, vec![1])]);
        assert!(cart_block(&cart).is_none());
    }

    #[test]
    fn the_flash_of_a_tnt_cart_covers_its_block() {
        let mut cart = entity(EntityKind::Minecart);
        cart.object_data = 3;
        cart.meta.ignite_cart();
        let flash = cart_flash(&cart).expect("flashing at fuse 80");
        let block = matrix(&cart_block(&cart).expect("tnt").1);
        let centre = |m: Mat4| m.transform_point3(Vec3::splat(0.5));
        assert!((centre(flash) - centre(block)).length() < 1e-5);
    }

    #[test]
    fn a_lit_tnt_cart_swells_at_the_end_of_its_fuse() {
        let mut cart = entity(EntityKind::Minecart);
        cart.object_data = 3;
        let size = |cart: &EntityRenderSnapshot| {
            matrix(&cart_block(cart).expect("tnt").1)
                .transform_vector3(Vec3::X)
                .length()
        };
        assert!((size(&cart) - 0.75).abs() < 1e-5, "unlit");
        cart.meta.ignite_cart();
        assert!((size(&cart) - 0.75).abs() < 1e-5, "80 ticks left");
        for _ in 0..78 {
            cart.meta.tick();
        }
        let grown = size(&cart) / 0.75;
        assert!(grown > 1.0 && grown <= 1.3 + 1e-5, "{grown}");
    }

    #[test]
    fn only_a_spawner_cart_showing_its_spawner_has_a_mob() {
        let mut cart = entity(EntityKind::Minecart);
        cart.object_data = 4;
        assert!(cart_spawner_frame(&cart).is_some());
        cart.object_data = 6;
        assert!(cart_spawner_frame(&cart).is_none());
    }

    #[test]
    fn the_jitter_of_a_cart_stays_within_two_thousandths() {
        for id in [0, 1, 7, 12_345, -3, i32::MAX] {
            let jitter = cart_jitter(id);
            assert!(jitter.abs().max_element() <= 0.002, "{id}: {jitter}");
        }
        assert_ne!(cart_jitter(1), cart_jitter(2));
    }

    #[test]
    fn only_a_falling_block_keeps_the_face_shade_of_the_world() {
        assert!(!takes_item_lights(EntityKind::FallingBlock));
        for kind in [
            EntityKind::Snowball,
            EntityKind::PrimedTnt,
            EntityKind::Minecart,
            EntityKind::SmallFireball,
        ] {
            assert!(takes_item_lights(kind), "{kind:?}");
        }
    }

    #[test]
    fn the_fuse_counts_down_from_eighty() {
        assert_eq!(fuse_remaining(0.0), 81.0);
        assert_eq!(fuse_remaining(80.0), 1.0);
    }

    #[test]
    fn a_falling_block_is_centred_with_its_bottom_at_the_entity() {
        let mut block = entity(EntityKind::FallingBlock);
        // Red sand: id 12 with data 1.
        block.object_data = 12 | (1 << 12);
        let (item, pose) = falling_block(&block);
        assert_eq!((item.item_id, item.damage), (12, 1));
        let centre = matrix(&pose).transform_point3(Vec3::splat(0.5));
        assert!(
            (centre - Vec3::new(10.0, 70.5, 4.0)).length() < 1e-4,
            "{centre}"
        );
    }

    /// Renders the objects. Set `UMICRAFT_OBJECT_DUMP` to a path to keep the
    /// PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn objects_in_the_world() {
        use crate::resources::models::ModelResources;
        use crate::scene::item_world::WorldItems;
        use umicraft_render::{Camera, Overlay, Renderer};

        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(960, 320).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let at = |id: i32, kind: EntityKind, x: f32, data: i32, age: f32| {
            let mut ent = EntityRenderSnapshot::new(EntityId(id), kind, Vec3::new(x, 64.0, 0.0));
            ent.object_data = data;
            ent.age = age;
            ent
        };
        let snapshots = [
            at(1, EntityKind::Snowball, -4.5, 0, 0.0),
            at(2, EntityKind::EnderPearl, -3.5, 0, 0.0),
            at(3, EntityKind::Potion, -2.5, 16_388, 0.0),
            at(4, EntityKind::ExpBottle, -1.5, 0, 0.0),
            at(5, EntityKind::LargeFireball, -0.3, 0, 0.0),
            at(6, EntityKind::SmallFireball, 1.0, 0, 0.0),
            at(7, EntityKind::FallingBlock, 2.2, 12, 0.0),
            at(8, EntityKind::PrimedTnt, 3.6, 0, 20.0),
            at(9, EntityKind::PrimedTnt, 4.8, 0, 76.0),
            at(10, EntityKind::Minecart, -1.8, 3, 0.0),
            at(11, EntityKind::Minecart, -3.0, 2, 0.0),
            at(12, EntityKind::Minecart, -4.2, 5, 0.0),
        ];
        let mut world = WorldItems::default();
        world.set_view(std::f32::consts::PI, 0.0);
        renderer.set_world_items(&world.build(&snapshots, &items).items);
        let mut camera = Camera::new(Vec3::new(0.0, 64.6, -7.5), std::f32::consts::PI, -0.02);
        camera.set_aspect(960, 320);
        camera.pin_render_origin(glam::DVec3::ZERO);
        renderer
            .render(&camera, &Overlay::new(960, 320))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_OBJECT_DUMP") {
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

    /// Renders every kind of minecart with its block: a lit TNT cart at the
    /// end of its fuse (swollen and flashing), a burning furnace cart, a
    /// spawner cart with its pig, a hopper, a command block, a chest and a
    /// custom wool tile. Set `UMICRAFT_CART_DUMP` to a path to keep the PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn minecarts_with_their_blocks() {
        use crate::resources::models::ModelResources;
        use crate::scene::entity::EntityTextures;
        use crate::scene::item_world::WorldItems;
        use umicraft_render::{Camera, Overlay, Renderer};

        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(960, 320).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let cart = |id: i32, x: f32, data: i32| {
            let mut ent = EntityRenderSnapshot::new(
                EntityId(id),
                EntityKind::Minecart,
                Vec3::new(x, 64.0, 0.0),
            );
            ent.object_data = data;
            ent.age = 40.0;
            ent.yaw = 90.0;
            ent
        };
        let mut tnt = cart(1, 4.2, 3);
        tnt.meta.ignite_cart();
        for _ in 0..78 {
            tnt.meta.tick();
        }
        let mut furnace = cart(2, 3.0, 2);
        furnace.meta = cart_meta(vec![watched(0, 16, vec![1])]);
        let mut wool = cart(7, -3.0, 0);
        wool.meta = cart_meta(vec![
            watched(2, 20, (35_i32 | 14 << 12).to_be_bytes().to_vec()),
            watched(0, 22, vec![1]),
        ]);
        let snapshots = [
            tnt,
            furnace,
            cart(3, 1.8, 4),
            cart(4, 0.6, 5),
            cart(5, -0.6, 6),
            cart(6, -1.8, 1),
            wool,
            cart(8, -4.2, 2),
        ];
        let mut world = WorldItems::default();
        world.set_view(std::f32::consts::PI, 0.0);
        renderer.set_world_items(&world.build(&snapshots, &items).items);
        EntityTextures::load(&renderer).mesh_and_upload(&snapshots, &mut renderer);
        let mut camera = Camera::new(Vec3::new(2.0, 65.6, -3.2), std::f32::consts::PI, -0.35);
        camera.set_aspect(960, 320);
        camera.pin_render_origin(glam::DVec3::ZERO);
        renderer
            .render(&camera, &Overlay::new(960, 320))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_CART_DUMP") {
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

    /// Renders primed TNT at ages 0, 2, ..., 18 (fuse 80 to 62), left to
    /// right: the flash is on for `fuse / 5` even, so the blocks go white,
    /// plain, plain, white, white, plain, ... Set `UMICRAFT_TNT_DUMP` to keep
    /// the PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn primed_tnt_flashes_every_five_ticks() {
        use crate::resources::models::ModelResources;
        use crate::scene::entity::EntityTextures;
        use crate::scene::item_world::WorldItems;
        use umicraft_render::{Camera, Overlay, Renderer};

        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(960, 240).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let snapshots: Vec<EntityRenderSnapshot> = (0..10)
            .map(|step| {
                let mut tnt = EntityRenderSnapshot::new(
                    EntityId(step + 1),
                    EntityKind::PrimedTnt,
                    Vec3::new(6.75 - step as f32 * 1.5, 64.0, 0.0),
                );
                tnt.age = step as f32 * 2.0;
                tnt
            })
            .collect();
        let mut world = WorldItems::default();
        world.set_view(std::f32::consts::PI, 0.0);
        renderer.set_world_items(&world.build(&snapshots, &items).items);
        EntityTextures::load(&renderer).mesh_and_upload(&snapshots, &mut renderer);
        let mut camera = Camera::new(Vec3::new(0.0, 64.5, -9.0), std::f32::consts::PI, 0.0);
        camera.set_aspect(960, 240);
        camera.pin_render_origin(glam::DVec3::ZERO);
        renderer
            .render(&camera, &Overlay::new(960, 240))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_TNT_DUMP") {
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
