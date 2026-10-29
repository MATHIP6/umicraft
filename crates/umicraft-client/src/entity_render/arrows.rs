// SPDX-License-Identifier: MIT OR Apache-2.0

//! Arrows stuck in the body of a player (MCP `LayerArrow`).
//!
//! The layer draws as many arrows as the entity metadata says, each at a spot
//! picked with `new Random(entityId)`: a part of the model, a box of that
//! part, then a point inside the box. The arrow points away from the body, in
//! the direction of that point seen from the centre of the box, and is the
//! model of a flying arrow (`RenderArrow`).

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{HumanoidModel, ModelPart, ModelVertex, TextureSlot};

use super::mobs::EntityLayer;
use super::player::posed_player;
use super::transform::part_frame;
use crate::java_random::JavaRandom;

/// The part of the posed model a [`Shape`] hangs on.
#[derive(Clone, Copy)]
enum Anchor {
    Head,
    Body,
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
    /// A part that no animation moves: its vanilla pivot `(x, y, z)`.
    Fixed([f32; 3]),
}

/// A box of `ModelBase.boxList`: where it hangs and its undilated extent in
/// the vanilla frame, from `(x1, y1, z1)` to `(x2, y2, z2)`.
#[derive(Clone, Copy)]
struct Shape {
    anchor: Anchor,
    min: [f32; 3],
    max: [f32; 3],
}

const fn shape(anchor: Anchor, min: [f32; 3], max: [f32; 3]) -> Shape {
    Shape { anchor, min, max }
}

const HEAD: Shape = shape(Anchor::Head, [-4.0, -8.0, -4.0], [4.0, 0.0, 4.0]);
const BODY: Shape = shape(Anchor::Body, [-4.0, 0.0, -2.0], [4.0, 12.0, 2.0]);
const LEG: [f32; 3] = [-2.0, 0.0, -2.0];
const LEG_END: [f32; 3] = [2.0, 12.0, 2.0];
const ARM_LEFT: Shape = shape(Anchor::LeftArm, [-1.0, -2.0, -2.0], [3.0, 10.0, 2.0]);
const ARM_RIGHT: Shape = shape(Anchor::RightArm, [-3.0, -2.0, -2.0], [1.0, 10.0, 2.0]);
const LEG_LEFT: Shape = shape(Anchor::LeftLeg, LEG, LEG_END);
const LEG_RIGHT: Shape = shape(Anchor::RightLeg, LEG, LEG_END);
/// The boxes that no animation moves, left over from the base biped model
/// (`ModelPlayer` replaces the arms and legs but the old ones stay in the list).
const OLD_LEFT_ARM: Shape = shape(Anchor::Fixed([5.0, 2.0, 0.0]), ARM_LEFT.min, ARM_LEFT.max);
const OLD_LEFT_LEG: Shape = shape(Anchor::Fixed([1.9, 12.0, 0.0]), LEG, LEG_END);
const DEADMAU5: Shape = shape(Anchor::Fixed([0.0; 3]), [-3.0, -6.0, -1.0], [3.0, 0.0, 0.0]);
const CAPE: Shape = shape(Anchor::Fixed([0.0; 3]), [-5.0, 0.0, -1.0], [5.0, 16.0, 0.0]);

/// `ModelPlayer(0.0F, false).boxList`, in construction order: the base biped,
/// the ears and the cape, then the new left arm and the wear layers.
const CLASSIC: [Shape; 16] = [
    HEAD,
    HEAD,
    BODY,
    ARM_RIGHT,
    OLD_LEFT_ARM,
    LEG_RIGHT,
    OLD_LEFT_LEG,
    DEADMAU5,
    CAPE,
    ARM_LEFT,
    ARM_LEFT,
    ARM_RIGHT,
    LEG_LEFT,
    LEG_LEFT,
    LEG_RIGHT,
    BODY,
];

/// The slim model builds both arms again, so its list has one more box and the
/// 4 pixel right arm of the base biped is left behind.
const SLIM_LEFT: Shape = shape(Anchor::LeftArm, [-1.0, -2.0, -2.0], [2.0, 10.0, 2.0]);
const SLIM_RIGHT: Shape = shape(Anchor::RightArm, [-2.0, -2.0, -2.0], [1.0, 10.0, 2.0]);
const OLD_RIGHT_ARM: Shape = shape(
    Anchor::Fixed([-5.0, 2.0, 0.0]),
    ARM_RIGHT.min,
    ARM_RIGHT.max,
);
const SLIM: [Shape; 17] = [
    HEAD,
    HEAD,
    BODY,
    OLD_RIGHT_ARM,
    OLD_LEFT_ARM,
    LEG_RIGHT,
    OLD_LEFT_LEG,
    DEADMAU5,
    CAPE,
    SLIM_LEFT,
    SLIM_RIGHT,
    SLIM_LEFT,
    SLIM_RIGHT,
    LEG_LEFT,
    LEG_LEFT,
    LEG_RIGHT,
    BODY,
];

/// Where an arrow sticks: a box and the three draws inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Draw {
    index: usize,
    inside: [f32; 3],
}

/// The draws of `count` arrows of the entity `id` over a list of `len` boxes:
/// `nextInt(len)` for the part, `nextInt(1)` for its only cube, then three
/// `nextFloat`.
fn draws(id: i32, count: u8, len: usize) -> Vec<Draw> {
    let mut random = JavaRandom::new(i64::from(id) as u64);
    (0..count)
        .map(|_| {
            let index = random.next_int(len as u32) as usize;
            random.next_int(1);
            let inside = [
                random.next_float(),
                random.next_float(),
                random.next_float(),
            ];
            Draw { index, inside }
        })
        .collect()
}

/// The arrow layers of `ent`: players only, as `RenderPlayer` is the only
/// renderer with a `LayerArrow` in 1.8.
pub(super) fn arrow_layers(ent: &EntityRenderSnapshot) -> Vec<EntityLayer> {
    let count = ent.meta.living.arrows;
    if ent.kind != EntityKind::Player || count == 0 {
        return Vec::new();
    }
    let (model, slim) = posed_player(ent);
    let table: &[Shape] = if slim { &SLIM } else { &CLASSIC };
    let arrow = arrow_model();
    let mut vertices = Vec::new();
    for draw in draws(ent.id.0, count, table.len()) {
        let shape = table[draw.index];
        let frame = part_frame(&anchor_part(&model, shape.anchor));
        let [f, f1, f2] = draw.inside;
        let at = Vec3::new(
            (shape.min[0] + (shape.max[0] - shape.min[0]) * f) / 16.0,
            (shape.min[1] + (shape.max[1] - shape.min[1]) * f1) / 16.0,
            (shape.min[2] + (shape.max[2] - shape.min[2]) * f2) / 16.0,
        );
        // The direction away from the centre of the box.
        let (dx, dy, dz) = (-(f * 2.0 - 1.0), -(f1 * 2.0 - 1.0), -(f2 * 2.0 - 1.0));
        let yaw = f64::from(dx).atan2(f64::from(dz)).to_degrees() as f32;
        let pitch = f64::from(dy)
            .atan2(f64::from((dx * dx + dz * dz).sqrt()))
            .to_degrees() as f32;
        let placed = frame
            * Mat4::from_translation(at)
            * Mat4::from_rotation_y((yaw - 90.0).to_radians())
            * Mat4::from_rotation_z(pitch.to_radians());
        for vertex in &arrow {
            vertices.push(ModelVertex {
                position: placed
                    .transform_point3(Vec3::from(vertex.position))
                    .to_array(),
                ..*vertex
            });
        }
    }
    vec![EntityLayer::new(TextureSlot::Arrow, vertices)]
}

/// The posed part an anchor stands for.
fn anchor_part(model: &HumanoidModel, anchor: Anchor) -> ModelPart {
    match anchor {
        Anchor::Head => model.head.clone(),
        Anchor::Body => model.body.clone(),
        Anchor::RightArm => model.right_arm.clone(),
        Anchor::LeftArm => model.left_arm.clone(),
        Anchor::RightLeg => model.right_leg.clone(),
        Anchor::LeftLeg => model.left_leg.clone(),
        // The models of this crate have the y axis up.
        Anchor::Fixed(pivot) => ModelPart::new([pivot[0], 24.0 - pivot[1], pivot[2]]),
    }
}

/// The arrow of `RenderArrow.doRender` in its own frame, in blocks: the tail
/// plate and four crossed quads of the shaft, on `arrow.png` (32 x 32).
fn arrow_model() -> Vec<ModelVertex> {
    let to_block = Mat4::from_rotation_x(45.0_f32.to_radians())
        * Mat4::from_scale(Vec3::splat(0.056_25))
        * Mat4::from_translation(Vec3::new(-4.0, 0.0, 0.0));
    let vertex = |matrix: &Mat4, x: f32, y: f32, z: f32, u: f32, v: f32| ModelVertex {
        position: matrix.transform_point3(Vec3::new(x, y, z)).to_array(),
        // The reference draws the arrow without lighting: a plain shade.
        normal: [0.0, 1.0, 0.0],
        uv: [u, v],
    };
    let mut out = Vec::new();
    let mut quad = |corners: [ModelVertex; 4]| {
        for index in [0, 1, 2, 0, 2, 3] {
            out.push(corners[index]);
        }
    };
    // The plate at the end of the shaft, seen from both sides.
    let (u0, u1, v0, v1) = (0.0, 0.156_25, 5.0 / 32.0, 10.0 / 32.0);
    quad([
        vertex(&to_block, -7.0, -2.0, -2.0, u0, v0),
        vertex(&to_block, -7.0, -2.0, 2.0, u1, v0),
        vertex(&to_block, -7.0, 2.0, 2.0, u1, v1),
        vertex(&to_block, -7.0, 2.0, -2.0, u0, v1),
    ]);
    quad([
        vertex(&to_block, -7.0, 2.0, -2.0, u0, v0),
        vertex(&to_block, -7.0, 2.0, 2.0, u1, v0),
        vertex(&to_block, -7.0, -2.0, 2.0, u1, v1),
        vertex(&to_block, -7.0, -2.0, -2.0, u0, v1),
    ]);
    // The shaft: four quads turned a quarter each about the arrow axis.
    for turn in 1..=4 {
        let around = to_block * Mat4::from_rotation_x((90.0 * turn as f32).to_radians());
        quad([
            vertex(&around, -8.0, -2.0, 0.0, 0.0, 0.0),
            vertex(&around, 8.0, -2.0, 0.0, 0.5, 0.0),
            vertex(&around, 8.0, 2.0, 0.0, 0.5, 5.0 / 32.0),
            vertex(&around, -8.0, 2.0, 0.0, 0.0, 5.0 / 32.0),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn player_with(arrows: u8) -> EntityRenderSnapshot {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), EntityKind::Player, glam::Vec3::ZERO);
        ent.meta.living.arrows = arrows;
        ent
    }

    #[test]
    fn the_draws_follow_the_reference_sequence() {
        // Values of `new Random(id)` printed by a JDK for a list of 16 boxes.
        let first = draws(1, 3, 16);
        assert_eq!(
            first.iter().map(|d| d.index).collect::<Vec<_>>(),
            vec![11, 0, 0]
        );
        assert_eq!(first[0].inside, [0.410_080_8, 0.407_439_77, 0.207_714_8]);
        let second = draws(5, 3, 16);
        assert_eq!(
            second.iter().map(|d| d.index).collect::<Vec<_>>(),
            vec![11, 15, 11]
        );
    }

    #[test]
    fn the_box_lists_have_the_length_of_model_player() {
        assert_eq!(CLASSIC.len(), 16);
        assert_eq!(SLIM.len(), 17);
    }

    #[test]
    fn a_player_without_arrows_and_the_other_kinds_draw_none() {
        assert!(arrow_layers(&player_with(0)).is_empty());
        let mut cow = player_with(3);
        cow.kind = EntityKind::Cow;
        assert!(arrow_layers(&cow).is_empty());
    }

    #[test]
    fn each_arrow_is_six_quads_on_the_arrow_texture() {
        // Two plate quads and four shaft quads, two triangles each.
        let layers = arrow_layers(&player_with(3));
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].slot, TextureSlot::Arrow);
        assert_eq!(layers[0].vertices.len(), 3 * 6 * 6);
    }

    #[test]
    fn the_arrows_stay_near_the_body() {
        // An arrow is 0.9 block long and may stick out of a part by that much.
        for vertex in &arrow_layers(&player_with(8))[0].vertices {
            let [x, y, z] = vertex.position;
            assert!(x.abs() < 1.5 && z.abs() < 1.5, "{x} {z}");
            assert!((-0.9..2.6).contains(&y), "{y}");
        }
    }
}
