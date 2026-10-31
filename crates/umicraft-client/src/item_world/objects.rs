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

use super::{sprite_model, world_light, ItemLayers};
use crate::entity_render::vehicles::wobble;
use crate::item_icons::ItemResources;

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
const MOB_SPAWNER: i16 = 52;
const HOPPER: i16 = 154;
const COMMAND_BLOCK: i16 = 137;
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
    let Some((model, quads)) = sprite_model(resources, item.item_id, item.damage) else {
        return;
    };
    let matrix = match pose {
        Pose::Block(matrix) => matrix,
        Pose::Sprite(frame) => frame * context_pose(model, DisplayContext::Ground),
    };
    let tints = resources.tints(item.item_id, item.damage, None);
    let style = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, false),
        tints: &tints,
    };
    layers.push(quads, &matrix, &style, false);
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
/// over the floor (`getDefaultDisplayTileOffset`). An empty cart and the
/// display tile of a cart edited with commands are not drawn.
pub(crate) fn cart_block(entity: &EntityRenderSnapshot) -> Option<(EntityItem, Pose)> {
    let (item_id, offset) = match entity.object_data {
        1 => (CHEST, 8),
        2 => (FURNACE, 6),
        3 => (TNT, 6),
        4 => (MOB_SPAWNER, 6),
        5 => (HOPPER, 1),
        6 => (COMMAND_BLOCK, 6),
        _ => return None,
    };
    // `RenderMinecart.doRender`: the cart's own frame (the model is drawn
    // 0.375 up, turned to its yaw and tipped by its pitch), then the block at
    // three quarters of its size, centred in the cart.
    let matrix = Mat4::from_translation(entity.position + Vec3::new(0.0, 0.375, 0.0))
        * Mat4::from_rotation_y((180.0 - entity.yaw).to_radians())
        * Mat4::from_rotation_z((-entity.pitch).to_radians())
        * Mat4::from_rotation_x(wobble(entity).to_radians())
        * Mat4::from_scale(Vec3::splat(0.75))
        * Mat4::from_translation(Vec3::new(-0.5, (offset - 8) as f32 / 16.0, 0.5))
        * Mat4::from_rotation_y(90.0_f32.to_radians());
    let item = EntityItem {
        item_id,
        damage: 0,
        count: 1,
        nbt: None,
    };
    Some((item, Pose::Block(matrix)))
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
    let remaining = fuse_remaining(entity.age);
    let swell = if remaining < 10.0 {
        let grown = (1.0 - remaining / 10.0).clamp(0.0, 1.0);
        1.0 + grown.powi(4) * 0.3
    } else {
        1.0
    };
    // `translate(-0.5, -0.5, 0.5)` then the quarter turn of
    // `renderBlockBrightness`.
    Pose::Block(
        Mat4::from_translation(entity.position + Vec3::new(0.0, 0.5, 0.0))
            * Mat4::from_scale(Vec3::splat(swell))
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
        assert!(
            (centre.y - expected_y).abs() < 1e-4,
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
        use crate::item_world::WorldItems;
        use crate::model_resources::ModelResources;
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
}
