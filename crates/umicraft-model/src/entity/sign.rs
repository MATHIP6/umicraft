// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sign model (MCP `ModelSign`) as `TileEntitySignRenderer` places it.
//!
//! A sign is a 24x12x2 board, and on a standing sign a 2x14x2 post, on a
//! 64x32 sheet. The renderer scales the model by `2/3`, so a board is 1 block
//! wide. The text is drawn in a second frame, [`sign_text_transform`].

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// The renderer's `f`: the scale of the model and the base of the text's.
const SIGN_SCALE: f32 = 0.666_666_7;

/// The turn, in degrees, of a standing sign of the given metadata (its
/// rotation, 0 to 15): `metadata * 360 / 16`. The renderer rotates by the
/// negative of it.
#[must_use]
pub fn standing_sign_degrees(metadata: u8) -> f32 {
    f32::from(metadata) * 360.0 / 16.0
}

/// The turn, in degrees, of a wall sign of the given facing metadata (2
/// north, 3 south, 4 west, 5 east; others as south, the reference's `f2 =
/// 0`). The renderer rotates by the negative of it; east is `-90` there and
/// `270` here, the same turn.
#[must_use]
pub fn wall_sign_degrees(metadata: u8) -> f32 {
    match metadata {
        2 => 180.0,
        4 => 90.0,
        5 => 270.0,
        _ => 0.0,
    }
}

/// The sign model, 64x32 texture sheet (`entity/sign.png`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SignModel;

impl SignModel {
    /// The sign in the block space of its tile.
    ///
    /// `standing` draws the board on its post, about the block centre at
    /// `y = 0.75 * 2/3`; otherwise the board hangs on a wall, moved by
    /// `translate(0, -0.3125, -0.4375)` against the wall and with no post
    /// (`signStick.showModel = false`). `rotation_degrees` comes from
    /// [`standing_sign_degrees`] or [`wall_sign_degrees`].
    #[must_use]
    pub fn emit(standing: bool, rotation_degrees: f32) -> Vec<ModelVertex> {
        let tex = [64.0, 32.0];
        // The reference's boxes are y-down; the crate's are y-up.
        let board = ModelPart::new([0.0; 3]).with_box(ModelBox::new(
            [-12.0, 2.0, -1.0],
            [24.0, 12.0, 2.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));
        let post = ModelPart::new([0.0; 3]).with_box(ModelBox::new(
            [-1.0, -12.0, -1.0],
            [2.0, 14.0, 2.0],
            [0.0, 14.0],
            tex,
            0.0,
        ));
        // `scale(f, -f, -f)` on the model in sixteenths; the y flip is
        // already in the boxes.
        let root = Self::frame(standing, rotation_degrees)
            * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0) * (SIGN_SCALE / 16.0));
        let mut out = Vec::new();
        board.emit(root, &mut out);
        if standing {
            post.emit(root, &mut out);
        }
        out
    }

    /// The renderer's frame before the model scale: the block centre raised
    /// to `0.75 * f`, turned by `-rotation`, and for a wall sign pushed to
    /// the wall.
    fn frame(standing: bool, rotation_degrees: f32) -> Mat4 {
        let placed = Mat4::from_translation(Vec3::new(0.5, 0.75 * SIGN_SCALE, 0.5))
            * Mat4::from_rotation_y((-rotation_degrees).to_radians());
        if standing {
            placed
        } else {
            placed * Mat4::from_translation(Vec3::new(0.0, -0.3125, -0.4375))
        }
    }
}

/// The matrix from the sign's text plane to block space.
///
/// The plane has x to the right and y down, in font pixels, with z toward
/// the reader; `drawString(s, -width / 2, j * 10 - lines * 5, 0)` puts line
/// `j` of `lines` (4) with its top at `y = 10 j - 20`, centred on `x = 0`.
/// The renderer, after the model, does `translate(0, 0.5 f, 0.07 f)` and
/// `scale(g, -g, g)` with `g = f / 64`.
#[must_use]
pub fn sign_text_transform(standing: bool, rotation_degrees: f32) -> Mat4 {
    let g = 0.015_625 * SIGN_SCALE;
    SignModel::frame(standing, rotation_degrees)
        * Mat4::from_translation(Vec3::new(0.0, 0.5 * SIGN_SCALE, 0.07 * SIGN_SCALE))
        * Mat4::from_scale(Vec3::new(g, -g, g))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(vertices: &[ModelVertex], axis: usize) -> (f32, f32) {
        vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), v| {
                (low.min(v.position[axis]), high.max(v.position[axis]))
            })
    }

    #[test]
    fn a_post_is_drawn_only_on_a_standing_sign() {
        assert_eq!(SignModel::emit(true, 0.0).len(), 72);
        assert_eq!(SignModel::emit(false, 0.0).len(), 36);
    }

    #[test]
    fn a_standing_sign_is_a_board_one_block_wide_on_a_post_to_the_ground() {
        let sign = SignModel::emit(true, 0.0);
        let (low, high) = extent(&sign, 0);
        assert!((high - low - 1.0).abs() < 1e-4 && (low - 0.0).abs() < 1e-4);
        let (low, high) = extent(&sign, 1);
        assert!(low.abs() < 1e-4, "{low}");
        assert!((high - (0.5 + 14.0 / 16.0 * SIGN_SCALE)).abs() < 1e-4);
    }

    #[test]
    fn a_wall_sign_sits_against_the_wall_it_faces_away_from() {
        // South (metadata 3, angle 0) hangs on the north wall of its block.
        let sign = SignModel::emit(false, wall_sign_degrees(3));
        let (_, far) = extent(&sign, 2);
        assert!(far < 0.11, "{far}");
        let north = SignModel::emit(false, wall_sign_degrees(2));
        let (near, _) = extent(&north, 2);
        assert!(near > 0.89, "{near}");
    }

    #[test]
    fn the_angles_follow_the_renderer() {
        assert_eq!(standing_sign_degrees(4), 90.0);
        let walls: Vec<f32> = (2..=5).map(wall_sign_degrees).collect();
        assert_eq!(walls, [180.0, 0.0, 90.0, 270.0]);
    }

    #[test]
    fn the_text_plane_hangs_on_the_front_of_a_standing_sign() {
        let plane = sign_text_transform(true, 0.0);
        let at = |x: f32, y: f32| plane.transform_point3(Vec3::new(x, y, 0.0));
        let g = 0.015_625 * SIGN_SCALE;
        let origin = at(0.0, 0.0);
        assert!(
            (origin - Vec3::new(0.5, 0.5 + 0.5 * SIGN_SCALE, 0.5 + 0.07 * SIGN_SCALE)).length()
                < 1e-5
        );
        // x runs right, y runs down, one font pixel is `g` blocks.
        assert!((at(10.0, 0.0) - origin - Vec3::new(10.0 * g, 0.0, 0.0)).length() < 1e-5);
        assert!((at(0.0, 10.0) - origin - Vec3::new(0.0, -10.0 * g, 0.0)).length() < 1e-5);
        // The four lines and the 90 px width lie within the 1 block board.
        assert!((at(45.0, 0.0).x - at(-45.0, 0.0).x).abs() < 1.0);
    }

    #[test]
    fn a_turned_text_plane_follows_the_board() {
        // Metadata 8 turns a standing sign half way round: the text moves to
        // the other side of the post.
        let plane = sign_text_transform(true, standing_sign_degrees(8));
        assert!(plane.transform_point3(Vec3::ZERO).z < 0.5);
    }
}
