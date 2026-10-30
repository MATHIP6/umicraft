// SPDX-License-Identifier: MIT OR Apache-2.0

//! Boats and minecarts (MCP `ModelBoat`, `RenderBoat`, `ModelMinecart`,
//! `RenderMinecart`).
//!
//! Both are models of five or six boxes drawn in the world frame at the
//! position of the entity, turned to its yaw. The boxes are those of the
//! reference with the y axis flipped: a pivot `(x, y, z)` is `(x, -y, z)` (these
//! two renderers do not lower the model by 1.5 blocks like the mobs do),
//! a box `(x, y, z, w, h, d)` starts at `(x, -(y + h), z)` and a rotation about
//! x is negated.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{ModelBox, ModelPart, ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;

use super::transform::x_mirror;

/// How high above the position of the entity the model is drawn.
const BOAT_LIFT: f32 = 0.25;
const MINECART_LIFT: f32 = 0.375;

/// A box of a 64 x 32 model from its vanilla definition.
fn vanilla_box(origin: [f32; 3], size: [f32; 3], uv: [f32; 2]) -> ModelBox {
    ModelBox::new(
        [origin[0], -(origin[1] + size[1]), origin[2]],
        size,
        uv,
        [64.0, 32.0],
        0.0,
    )
}

/// A part with the vanilla pivot `(x, y, z)` and the rotation `(rx, ry)` about
/// x and y (the rotation about x of the vanilla frame is the opposite here).
/// The origin of the model is the entity itself, so only the y axis flips.
fn part(pivot: [f32; 3], rx: f32, ry: f32, shape: ModelBox) -> ModelPart {
    let mut part = ModelPart::new([pivot[0], -pivot[1], pivot[2]]).with_box(shape);
    part.rotation = [-rx, ry, 0.0];
    part
}

/// `ModelBoat`: a floor and four sides.
fn boat_parts() -> Vec<ModelPart> {
    let side = || vanilla_box([-10.0, -7.0, -1.0], [20.0, 6.0, 2.0], [0.0, 0.0]);
    vec![
        part(
            [0.0, 4.0, 0.0],
            FRAC_PI_2,
            0.0,
            vanilla_box([-12.0, -8.0, -3.0], [24.0, 16.0, 4.0], [0.0, 8.0]),
        ),
        part([-11.0, 4.0, 0.0], 0.0, PI * 1.5, side()),
        part([11.0, 4.0, 0.0], 0.0, FRAC_PI_2, side()),
        part([0.0, 4.0, -9.0], 0.0, PI, side()),
        part([0.0, 4.0, 9.0], 0.0, 0.0, side()),
    ]
}

/// `ModelMinecart`: a floor, four sides and the inside of the floor, which
/// `render` lifts by the 0.1 of its `ageInTicks` argument.
fn minecart_parts() -> Vec<ModelPart> {
    let side = || vanilla_box([-8.0, -9.0, -1.0], [16.0, 8.0, 2.0], [0.0, 0.0]);
    vec![
        part(
            [0.0, 4.0, 0.0],
            FRAC_PI_2,
            0.0,
            vanilla_box([-10.0, -8.0, -1.0], [20.0, 16.0, 2.0], [0.0, 10.0]),
        ),
        part([-9.0, 4.0, 0.0], 0.0, PI * 1.5, side()),
        part([9.0, 4.0, 0.0], 0.0, FRAC_PI_2, side()),
        part([0.0, 4.0, -7.0], 0.0, PI, side()),
        part([0.0, 4.0, 7.0], 0.0, 0.0, side()),
        part(
            [0.0, 4.1, 0.0],
            -FRAC_PI_2,
            0.0,
            vanilla_box([-9.0, -7.0, -1.0], [18.0, 14.0, 1.0], [44.0, 10.0]),
        ),
    ]
}

/// The mesh of a model of parts, in blocks.
fn mesh(parts: &[ModelPart]) -> Vec<ModelVertex> {
    let mut vertices = Vec::new();
    let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
    for part in parts {
        part.emit(root, &mut vertices);
    }
    vertices
}

/// Adds the boat or the minecart `ent` to `batches` when it is one.
pub(super) fn add_vehicle(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    light: u32,
) {
    let (slot, parts, lift, tilt) = match ent.kind {
        EntityKind::Boat => (TextureSlot::Boat, boat_parts(), BOAT_LIFT, 0.0),
        // The cart follows the slope of its track: `rotate(-pitch, Z)`.
        EntityKind::Minecart => (
            TextureSlot::Minecart,
            minecart_parts(),
            MINECART_LIFT,
            -ent.pitch.to_radians(),
        ),
        EntityKind::EnderCrystal => return add_crystal(batches, ent, light),
        _ => return,
    };
    let placed = Mat4::from_translation(ent.position + Vec3::new(0.0, lift, 0.0))
        * Mat4::from_rotation_y((180.0 - ent.yaw).to_radians())
        * Mat4::from_rotation_z(tilt)
        * x_mirror();
    push(
        batches.entry(slot).or_default(),
        mesh(&parts),
        placed,
        light,
    );
}

/// Carries `vertices` through `placed` into `out`.
fn push(out: &mut Vec<EntityVertex>, vertices: Vec<ModelVertex>, placed: Mat4, light: u32) {
    out.extend(vertices.into_iter().map(|vertex| {
        EntityVertex {
            position: placed
                .transform_point3(Vec3::from(vertex.position))
                .to_array(),
            normal: placed
                .transform_vector3(Vec3::from(vertex.normal))
                .normalize_or_zero()
                .to_array(),
            uv: vertex.uv,
            light,
        }
    }));
}

/// One box of the crystal model, from its vanilla definition.
fn crystal_box(origin: [f32; 3], size: [f32; 3], uv: [f32; 2]) -> Vec<ModelVertex> {
    mesh(&[ModelPart::new([0.0; 3]).with_box(vanilla_box(origin, size, uv))])
}

/// `RenderEnderCrystal` and `ModelEnderCrystal`: a base under two glass cubes
/// and a core, each turned and tilted inside the one before, bobbing over the
/// base. The renderer draws without the `scale(-1, -1, 1)` of the mobs, so
/// the y flip of this crate's models is undone and nothing is mirrored.
fn add_crystal(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    light: u32,
) {
    // `innerRotation` starts at a random number, which a client never sees:
    // the id spreads the crystals instead.
    let age = ent.age + ent.id.0.rem_euclid(100_000) as f32;
    let wave = (age * 0.2).sin() / 2.0 + 0.5;
    let bob = (wave * wave + wave) * 0.2;
    let tilt = Mat4::from_axis_angle(Vec3::new(1.0, 0.0, 1.0).normalize(), 60.0_f32.to_radians());
    let turn = Mat4::from_rotation_y((age * 3.0).to_radians());
    let shrink = Mat4::from_scale(Vec3::splat(0.875));
    let unflip = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let root = Mat4::from_translation(ent.position)
        * Mat4::from_scale(Vec3::splat(2.0))
        * Mat4::from_translation(Vec3::new(0.0, -0.5, 0.0));
    let outer = root * turn * Mat4::from_translation(Vec3::new(0.0, 0.8 + bob, 0.0)) * tilt;
    let inner = outer * shrink * tilt * turn;
    let core = inner * shrink * tilt * turn;
    let out = batches.entry(TextureSlot::EnderCrystal).or_default();
    let glass = || crystal_box([-4.0; 3], [8.0; 3], [0.0, 0.0]);
    push(
        out,
        crystal_box([-6.0, 0.0, -6.0], [12.0, 4.0, 12.0], [0.0, 16.0]),
        root * unflip,
        light,
    );
    push(out, glass(), outer * unflip, light);
    push(out, glass(), inner * unflip, light);
    push(
        out,
        crystal_box([-4.0; 3], [8.0; 3], [32.0, 0.0]),
        core * unflip,
        light,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn at(kind: EntityKind, yaw: f32, pitch: f32) -> EntityRenderSnapshot {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(5.0, 64.0, 5.0));
        ent.yaw = yaw;
        ent.pitch = pitch;
        ent
    }

    fn vertices(ent: &EntityRenderSnapshot, slot: TextureSlot) -> Vec<EntityVertex> {
        let mut batches = HashMap::new();
        add_vehicle(&mut batches, ent, 0xFF);
        batches.remove(&slot).unwrap_or_default()
    }

    fn extent(vertices: &[EntityVertex], axis: usize) -> (f32, f32) {
        let values = vertices.iter().map(|v| v.position[axis]);
        (
            values.clone().fold(f32::MAX, f32::min),
            values.fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn a_boat_is_five_boxes_and_a_minecart_six() {
        assert_eq!(
            vertices(&at(EntityKind::Boat, 0.0, 0.0), TextureSlot::Boat).len(),
            5 * 36
        );
        assert_eq!(
            vertices(&at(EntityKind::Minecart, 0.0, 0.0), TextureSlot::Minecart).len(),
            6 * 36
        );
    }

    #[test]
    fn other_kinds_are_not_vehicles() {
        let mut batches = HashMap::new();
        add_vehicle(&mut batches, &at(EntityKind::Pig, 0.0, 0.0), 0);
        assert!(batches.is_empty());
    }

    #[test]
    fn a_boat_is_24_pixels_long_and_20_wide() {
        // Yaw 0: the long side (24) is along x, the width (20) along z.
        let boat = vertices(&at(EntityKind::Boat, 0.0, 0.0), TextureSlot::Boat);
        let (x0, x1) = extent(&boat, 0);
        let (z0, z1) = extent(&boat, 2);
        assert!((x1 - x0 - 24.0 / 16.0).abs() < 0.05, "{}", x1 - x0);
        assert!((z1 - z0 - 20.0 / 16.0).abs() < 0.05, "{}", z1 - z0);
        // A quarter turn swaps them.
        let turned = vertices(&at(EntityKind::Boat, 90.0, 0.0), TextureSlot::Boat);
        let (x0, x1) = extent(&turned, 0);
        assert!((x1 - x0 - 20.0 / 16.0).abs() < 0.05, "{}", x1 - x0);
    }

    #[test]
    fn the_boat_and_the_cart_sit_around_their_lifted_origin() {
        let boat = vertices(&at(EntityKind::Boat, 0.0, 0.0), TextureSlot::Boat);
        let cart = vertices(&at(EntityKind::Minecart, 0.0, 0.0), TextureSlot::Minecart);
        // The boat spans 10 pixels from 7 below to 3 above its origin, 0.25
        // over the position: from 0.19 under it to 0.44 over it.
        let (low, high) = extent(&boat, 1);
        assert!(
            (low - (64.0 + BOAT_LIFT - 7.0 / 16.0)).abs() < 0.02,
            "{low}"
        );
        assert!(
            (high - (64.0 + BOAT_LIFT + 3.0 / 16.0)).abs() < 0.02,
            "{high}"
        );
        // The cart floor is 5 pixels under its origin, 0.375 over the position.
        let (low, _) = extent(&cart, 1);
        assert!(
            (low - (64.0 + MINECART_LIFT - 5.0 / 16.0)).abs() < 0.02,
            "{low}"
        );
    }

    #[test]
    fn a_cart_tips_with_the_slope_of_its_track() {
        let flat = vertices(&at(EntityKind::Minecart, 0.0, 0.0), TextureSlot::Minecart);
        let sloped = vertices(&at(EntityKind::Minecart, 0.0, 30.0), TextureSlot::Minecart);
        assert_ne!(extent(&flat, 1), extent(&sloped, 1));
    }

    #[test]
    fn a_crystal_is_a_base_and_three_cubes_standing_on_its_position() {
        let crystal = vertices(
            &at(EntityKind::EnderCrystal, 0.0, 0.0),
            TextureSlot::EnderCrystal,
        );
        assert_eq!(crystal.len(), 4 * 36);
        // The base is 4 pixels thick, doubled, and `translate(0, -0.5, 0)`
        // comes after the doubling: it spans from one block under the position
        // to half a block under it, and nothing hangs below it.
        let (low, _) = extent(&crystal, 1);
        assert!((low - (64.0 - 1.0)).abs() < 1e-4, "{low}");
    }

    #[test]
    fn a_crystal_bobs_and_spins_with_its_age() {
        let mut ent = at(EntityKind::EnderCrystal, 0.0, 0.0);
        let before = vertices(&ent, TextureSlot::EnderCrystal);
        ent.age = 7.0;
        let after = vertices(&ent, TextureSlot::EnderCrystal);
        assert_ne!(extent(&before, 1).1, extent(&after, 1).1);
        assert_ne!(extent(&before, 0), extent(&after, 0));
    }

    /// Renders a boat and minecarts. Set `UMICRAFT_VEHICLE_DUMP` to a path to
    /// keep the PNG.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn vehicles_in_the_world() {
        use crate::entity_render::EntityTextures;
        use umicraft_render::{Camera, Overlay, Renderer};

        let mut renderer = Renderer::new_offscreen(800, 320).expect("renderer");
        let place = |id: i32, kind: EntityKind, x: f32, yaw: f32, pitch: f32| {
            let mut ent = EntityRenderSnapshot::new(EntityId(id), kind, Vec3::new(x, 64.0, 0.0));
            ent.yaw = yaw;
            ent.pitch = pitch;
            ent
        };
        let mut snapshots = vec![
            place(1, EntityKind::Boat, -3.0, 0.0, 0.0),
            place(2, EntityKind::Boat, -0.8, 90.0, 0.0),
            place(3, EntityKind::Minecart, 1.3, 0.0, 0.0),
            place(4, EntityKind::Minecart, 3.2, 90.0, 30.0),
            place(5, EntityKind::EnderCrystal, 5.2, 0.0, 0.0),
        ];
        // Orbs of growing value, a few ticks apart in colour.
        for (at, xp) in [1, 3, 7, 17, 37, 73, 149, 307, 617, 1237, 2477]
            .into_iter()
            .enumerate()
        {
            let mut orb = place(
                10 + at as i32,
                EntityKind::XpOrb,
                -4.0 + at as f32 * 0.8,
                0.0,
                0.0,
            );
            orb.position.y = 62.5;
            orb.object_data = xp;
            orb.age = at as f32 * 6.0;
            snapshots.push(orb);
        }
        for (id, kind, x, yaw) in [
            (40, EntityKind::WitherSkull, 3.0, 20.0),
            (41, EntityKind::LeashKnot, 1.0, 0.0),
        ] {
            let mut small = place(id, kind, x, yaw, 15.0);
            small.position = Vec3::new(x, 64.0, -3.0);
            snapshots.push(small);
        }
        // Two paintings on the far side, facing the camera (north, yaw 180).
        for (id, title, x) in [(30, "Skeleton", -2.0), (31, "Pigscene", 2.5)] {
            let mut painting = place(id, EntityKind::Painting, x, 180.0, 0.0);
            painting.position = Vec3::new(x, 66.0, 4.0);
            painting.object_data =
                umicraft_game::entity::painting::art_index(title).unwrap() as i32;
            snapshots.push(painting);
        }
        let textures = EntityTextures::load(&renderer);
        textures.set_view_yaw(std::f32::consts::PI);
        textures.set_view_pitch(-0.12);
        textures.mesh_and_upload(&snapshots, &mut renderer);
        let mut camera = Camera::new(Vec3::new(1.0, 65.4, -9.0), std::f32::consts::PI, -0.12);
        camera.set_aspect(800, 320);
        renderer
            .render(&camera, &Overlay::new(800, 320))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_VEHICLE_DUMP") {
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
