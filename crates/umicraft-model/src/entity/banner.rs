// SPDX-License-Identifier: MIT OR Apache-2.0

//! The banner model (MCP `ModelBanner`) as `TileEntityBannerRenderer` places it.
//!
//! A banner is a 20x40x1 cloth hanging from a 20x2x2 bar, and on a standing
//! banner a 2x42x2 pole, on a 64x64 sheet. The renderer scales the model by
//! `2/3`. The cloth sways about the bar by a small angle that changes with
//! time, [`banner_sway`].

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// The renderer's `f`: the scale of the model.
const BANNER_SCALE: f32 = 0.666_666_7;
/// Height of the bar above the model origin, in model pixels
/// (`bannerSlate.rotationPointY = -32`, y down in the reference).
const BAR_HEIGHT: f32 = 32.0;

/// The tilt of the cloth, in radians, `phase` ticks into its sway:
/// `(-0.0125 + 0.01 cos(phase * pi * 0.02)) * pi`, the value the renderer
/// gives `bannerSlate.rotateAngleX`. `phase` is
/// `x * 7 + y * 9 + z * 13 + world time + partial tick`, so neighbouring
/// banners do not move in step.
#[must_use]
pub fn banner_sway(phase: f32) -> f32 {
    (-0.0125 + 0.01 * (phase * PI * 0.02).cos()) * PI
}

/// The banner model, 64x64 texture sheet (`entity/banner_base.png` composed
/// with the patterns).
#[derive(Debug, Clone, Copy, Default)]
pub struct BannerModel;

impl BannerModel {
    /// The banner in the block space of its tile.
    ///
    /// `standing` draws the pole and raises the bar to the block centre at
    /// `y = 0.75 * 2/3`; otherwise the banner hangs on a wall, at
    /// `y = -0.25 * 2/3` moved by `translate(0, -0.3125, -0.4375)` against the
    /// wall, with no pole (`bannerStand.showModel = false`). The bar is drawn
    /// in both cases, as `renderBanner` does. `rotation_degrees` is the same
    /// turn as for signs (`standing_sign_degrees`, `wall_sign_degrees`: the
    /// renderer uses the same formulas) and `sway_angle` comes from
    /// [`banner_sway`].
    #[must_use]
    pub fn emit(standing: bool, rotation_degrees: f32, sway_angle: f32) -> Vec<ModelVertex> {
        let tex = [64.0, 64.0];
        // The reference's boxes are y-down; the crate's are y-up, so a box
        // `y..y + h` there is `-(y + h)..-y` here and a rotation about x
        // changes sign.
        let mut cloth = ModelPart::new([0.0, BAR_HEIGHT, 0.0]).with_box(ModelBox::new(
            [-10.0, -40.0, -2.0],
            [20.0, 40.0, 1.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));
        cloth.rotation[0] = -sway_angle;
        let pole = ModelPart::new([0.0; 3]).with_box(ModelBox::new(
            [-1.0, -12.0, -1.0],
            [2.0, 42.0, 2.0],
            [44.0, 0.0],
            tex,
            0.0,
        ));
        let bar = ModelPart::new([0.0; 3]).with_box(ModelBox::new(
            [-10.0, 30.0, -1.0],
            [20.0, 2.0, 2.0],
            [0.0, 42.0],
            tex,
            0.0,
        ));
        // `scale(f, -f, -f)` on the model in sixteenths; the y flip is
        // already in the boxes.
        let root = Self::frame(standing, rotation_degrees)
            * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0) * (BANNER_SCALE / 16.0));
        let mut out = Vec::new();
        cloth.emit(root, &mut out);
        if standing {
            pole.emit(root, &mut out);
        }
        bar.emit(root, &mut out);
        out
    }

    /// The renderer's frame before the model scale.
    fn frame(standing: bool, rotation_degrees: f32) -> Mat4 {
        let turn = Mat4::from_rotation_y((-rotation_degrees).to_radians());
        if standing {
            Mat4::from_translation(Vec3::new(0.5, 0.75 * BANNER_SCALE, 0.5)) * turn
        } else {
            Mat4::from_translation(Vec3::new(0.5, -0.25 * BANNER_SCALE, 0.5))
                * turn
                * Mat4::from_translation(Vec3::new(0.0, -0.3125, -0.4375))
        }
    }
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
    fn the_pole_is_drawn_only_on_a_standing_banner() {
        assert_eq!(BannerModel::emit(true, 0.0, 0.0).len(), 3 * 36);
        assert_eq!(BannerModel::emit(false, 0.0, 0.0).len(), 2 * 36);
    }

    #[test]
    fn a_standing_banner_is_two_blocks_tall_from_the_ground_and_one_wide() {
        let banner = BannerModel::emit(true, 0.0, 0.0);
        let (low, high) = extent(&banner, 1);
        assert!(low.abs() < 1e-4, "{low}");
        // The pole ends at the top of the bar, 32 px above the origin.
        assert!((high - (0.5 + 32.0 / 16.0 * BANNER_SCALE)).abs() < 1e-4);
        let (left, right) = extent(&banner, 0);
        assert!((right - left - 20.0 / 16.0 * BANNER_SCALE).abs() < 1e-4);
    }

    #[test]
    fn the_cloth_hangs_from_the_bar() {
        let banner = BannerModel::emit(true, 0.0, 0.0);
        // The first 36 vertices are the cloth: 40 px long below the bar.
        let (low, high) = extent(&banner[..36], 1);
        let bar = 0.5 + 32.0 / 16.0 * BANNER_SCALE;
        assert!((high - bar).abs() < 1e-4);
        assert!((bar - low - 40.0 / 16.0 * BANNER_SCALE).abs() < 1e-4);
    }

    #[test]
    fn the_sway_swings_the_foot_of_the_cloth_not_the_bar() {
        let still = BannerModel::emit(true, 0.0, 0.0);
        let swung = BannerModel::emit(true, 0.0, 0.2);
        assert_ne!(still[..36], swung[..36]);
        assert_eq!(still[36..], swung[36..]);
        // The top edge stays on the bar.
        assert!((extent(&still[..36], 1).1 - extent(&swung[..36], 1).1).abs() < 0.03);
    }

    #[test]
    fn a_wall_banner_hangs_against_the_wall_it_faces_away_from() {
        // Angle 0 (south) is on the north wall of its block.
        let (_, far) = extent(&BannerModel::emit(false, 0.0, 0.0), 2);
        assert!(far < 0.2, "{far}");
        let (near, _) = extent(&BannerModel::emit(false, 180.0, 0.0), 2);
        assert!(near > 0.8, "{near}");
    }

    #[test]
    fn the_sway_follows_the_renderer() {
        assert!((banner_sway(0.0) - (-0.0025 * PI)).abs() < 1e-6);
        // Half a period (50 ticks) later the cosine is -1.
        assert!((banner_sway(50.0) - (-0.0225 * PI)).abs() < 1e-5);
        assert!(banner_sway(1234.5).abs() <= 0.0225 * PI);
    }
}
