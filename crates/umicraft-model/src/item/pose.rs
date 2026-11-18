// SPDX-License-Identifier: MIT OR Apache-2.0

//! The matrix chains `RenderItem` applies around an item model.
//!
//! Every function returns the matrix that takes a point of the baked model
//! (a block-sized cube, `0.0..=1.0`) to the frame the caller then positions:
//! the GUI box, or the origin of the world, hand or entity the caller is
//! drawing at. The chains are the GL call sequences of MCP `RenderItem`
//! (`setupGuiTransform`, `renderItem`, `preTransform`,
//! `renderItemModelTransform`); each GL call multiplies on the right.

use glam::{Mat4, Vec3};

use crate::display::DisplayContext;

use super::ItemModel;

/// Pixels of the GUI slot an icon fills.
pub const GUI_ICON_SIZE: f32 = 16.0;

/// `RenderItem.renderItem`: half size, then centre the model on the origin.
fn centre_model() -> Mat4 {
    Mat4::from_scale(Vec3::splat(0.5)) * Mat4::from_translation(Vec3::splat(-0.5))
}

/// Model space to the 16x16 GUI icon box (origin top-left, y down, z depth).
///
/// `setupGuiTransform` places the item at the slot centre, flips z, and
/// turns lit blocks to the isometric view (210 about X, -135 about Y) or
/// flat sprites upside down (180 about X); the model's own `gui` pose and
/// [`centre_model`] follow.
#[must_use]
pub fn gui_pose(model: &ItemModel) -> Mat4 {
    let view = if model.gui_3d {
        Mat4::from_scale(Vec3::splat(40.0))
            * Mat4::from_rotation_x(210.0_f32.to_radians())
            * Mat4::from_rotation_y((-135.0_f32).to_radians())
    } else {
        Mat4::from_scale(Vec3::splat(64.0)) * Mat4::from_rotation_x(180.0_f32.to_radians())
    };
    Mat4::from_translation(Vec3::new(GUI_ICON_SIZE / 2.0, GUI_ICON_SIZE / 2.0, 0.0))
        * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
        * Mat4::from_scale(Vec3::splat(0.5))
        * view
        * model.transform(DisplayContext::Gui).matrix()
        * centre_model()
}

/// Model space to the frame of a world, hand or frame context.
///
/// `renderItemModelTransform`: flat sprites are doubled
/// (`preTransform`), then the model's pose for `context` and
/// [`centre_model`].
#[must_use]
pub fn context_pose(model: &ItemModel, context: DisplayContext) -> Mat4 {
    let pre = if model.gui_3d {
        Mat4::IDENTITY
    } else {
        Mat4::from_scale(Vec3::splat(2.0))
    };
    pre * model.transform(context).matrix() * centre_model()
}

/// Model space to the frame of an item held in a hand
/// (`ItemRenderer.renderItem` and `renderItemModelTransform`).
///
/// Both branches of the reference double the size: lit blocks in
/// `renderItem`, flat sprites in `preTransform`.
#[must_use]
pub fn held_pose(model: &ItemModel, context: DisplayContext) -> Mat4 {
    Mat4::from_scale(Vec3::splat(2.0)) * model.transform(context).matrix() * centre_model()
}

/// The depth of the front face of a flat sprite in model space
/// (`ItemModelGenerator`: the sprite spans `7.5..8.5` pixels).
const SPRITE_FRONT: f32 = 7.5 / 16.0;

/// Model space to the frame of a flat sprite held in the first-person hand as
/// the 1.7 client drew it (ADR-0079), in place of [`held_pose`].
///
/// 1.7.10 `ItemRenderer.renderItem` has no item models: its 2D branch
/// (lines 104-113) places every sprite with the same calls, `translate(0,
/// -0.3, 0)`, `scale(1.5)`, `rotate(50, Y)`, `rotate(335, Z)`,
/// `translate(-0.9375, -0.0625, 0)`, then `renderItemIn2D` draws a unit
/// square whose `x` runs against `u` and whose depth goes from 0 to
/// -1/16. The first factor below takes the baked sprite to that square: a
/// half turn about the vertical axis, which mirrors `u` and keeps the faces
/// facing out. `turned` is `Item.shouldRotateAroundWhenRendering` (the
/// fishing rod and the carrot on a stick), a half turn about Y first.
#[must_use]
pub fn legacy_flat_held_pose(turned: bool) -> Mat4 {
    let turn = if turned {
        Mat4::from_rotation_y(std::f32::consts::PI)
    } else {
        Mat4::IDENTITY
    };
    let to_square = Mat4::from_translation(Vec3::new(1.0, 0.0, SPRITE_FRONT))
        * Mat4::from_scale(Vec3::new(-1.0, 1.0, -1.0));
    turn * Mat4::from_translation(Vec3::new(0.0, -0.3, 0.0))
        * Mat4::from_scale(Vec3::splat(1.5))
        * Mat4::from_rotation_y(50.0_f32.to_radians())
        * Mat4::from_rotation_z(335.0_f32.to_radians())
        * Mat4::from_translation(Vec3::new(-0.9375, -0.0625, 0.0))
        * to_square
}

/// Model space to the frame of a dropped item (`RenderEntityItem`).
///
/// Lit blocks are halved first (the extra `scale(0.5)` of the 3D branch);
/// flat sprites are not doubled here, unlike [`context_pose`], because the
/// entity renderer calls `renderItem` directly.
#[must_use]
pub fn ground_pose(model: &ItemModel) -> Mat4 {
    let pre = if model.gui_3d {
        Mat4::from_scale(Vec3::splat(0.5))
    } else {
        Mat4::IDENTITY
    };
    pre * model.transform(DisplayContext::Ground).matrix() * centre_model()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::DisplayTransforms;
    use crate::item::ItemGeometry;

    fn model(gui_3d: bool) -> ItemModel {
        ItemModel {
            geometry: ItemGeometry::Entity,
            display: DisplayTransforms::default(),
            gui_3d,
            particle: None,
        }
    }

    #[test]
    fn the_1_7_sprite_sits_where_1_7_drew_its_square() {
        let pose = legacy_flat_held_pose(false);
        // The bottom left of the picture (u 0, v 1) is the corner `x = 1`
        // of the 1.7 square, the top right (u 1, v 0) its corner `y = 1`.
        let square = |x: f32, y: f32| {
            {
                Mat4::from_translation(Vec3::new(0.0, -0.3, 0.0))
                    * Mat4::from_scale(Vec3::splat(1.5))
                    * Mat4::from_rotation_y(50.0_f32.to_radians())
                    * Mat4::from_rotation_z(335.0_f32.to_radians())
                    * Mat4::from_translation(Vec3::new(-0.9375, -0.0625, 0.0))
            }
            .transform_point3(Vec3::new(x, y, 0.0))
        };
        let front = |u: f32, v: f32| pose.transform_point3(Vec3::new(u, 1.0 - v, SPRITE_FRONT));
        assert!((front(0.0, 1.0) - square(1.0, 0.0)).length() < 1e-5);
        assert!((front(1.0, 0.0) - square(0.0, 1.0)).length() < 1e-5);
        // A pure placement: no mirror, so the faces keep their winding.
        assert!(pose.determinant() > 0.0);
        let turned = legacy_flat_held_pose(true).transform_point3(Vec3::ONE);
        let plain = pose.transform_point3(Vec3::ONE);
        assert!((turned - Vec3::new(-plain.x, plain.y, -plain.z)).length() < 1e-5);
    }

    #[test]
    fn a_flat_sprite_fills_the_icon_box() {
        let pose = gui_pose(&model(false));
        let corner = pose.transform_point3(Vec3::new(0.0, 0.0, 0.5));
        let opposite = pose.transform_point3(Vec3::new(1.0, 1.0, 0.5));
        // 16 px wide, centred on the slot, y flipped by the 180 degree turn.
        assert!(
            (corner - Vec3::new(0.0, 16.0, 0.0)).length() < 1e-3,
            "{corner}"
        );
        assert!(
            (opposite - Vec3::new(16.0, 0.0, 0.0)).length() < 1e-3,
            "{opposite}"
        );
    }

    #[test]
    fn a_lit_block_stays_inside_the_icon_box() {
        let pose = gui_pose(&model(true));
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 1.0] {
                    let point = pose.transform_point3(Vec3::new(x, y, z));
                    assert!((-1.0..=17.0).contains(&point.x), "{point}");
                    assert!((-1.0..=17.0).contains(&point.y), "{point}");
                }
            }
        }
    }

    #[test]
    fn a_dropped_flat_sprite_is_half_a_block_and_a_block_a_quarter() {
        let flat = ground_pose(&model(false));
        let block = ground_pose(&model(true));
        assert!((flat.transform_vector3(Vec3::X).length() - 0.5).abs() < 1e-5);
        assert!((block.transform_vector3(Vec3::X).length() - 0.25).abs() < 1e-5);
    }

    #[test]
    fn a_held_item_is_doubled_whatever_its_kind() {
        for gui_3d in [false, true] {
            let pose = held_pose(&model(gui_3d), DisplayContext::FirstPerson);
            assert!((pose.transform_vector3(Vec3::X).length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn flat_items_are_doubled_in_the_world_and_blocks_are_not() {
        let flat = context_pose(&model(false), DisplayContext::Ground);
        let block = context_pose(&model(true), DisplayContext::Ground);
        let centre = Vec3::splat(0.5);
        assert!(flat.transform_point3(centre).length() < 1e-5);
        assert!((flat.transform_vector3(Vec3::X).length() - 1.0).abs() < 1e-5);
        assert!((block.transform_vector3(Vec3::X).length() - 0.5).abs() < 1e-5);
    }
}
