// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sky dome and the void plane below the horizon
//! (`RenderGlobal.generateSky` / `generateSky2` / `renderSky`).
//!
//! The reference builds one flat grid of 64-block squares from -384 to +448
//! and calls it twice: at `y = +16` for the sky and at `y = -16` for the void
//! below the horizon. Both are the same grid at a different height, so this
//! module builds the grid once at `y = 0` and the frame places it with a
//! translation. The winding flip of the lower list is dropped because the
//! pass draws without face culling.

use glam::Mat4;

use super::vertex::{quad, SkyVertex};

/// First grid line in blocks (`for k = -384`).
const FIRST: i32 = -384;
/// Last grid line start in blocks (`k <= 384`).
const LAST: i32 = 384;
/// Square size in blocks.
const STEP: i32 = 64;
/// Height of the sky list above the eye.
pub(super) const SKY_HEIGHT: f32 = 16.0;
/// Height of the void list below the eye, before its translations.
pub(super) const VOID_HEIGHT: f32 = -16.0;
/// `getHorizon()` of a default world: the plane the void dome lies on.
const HORIZON_Y: f32 = 63.0;
/// `getHorizon()` of a superflat world (`WorldType.FLAT`).
const FLAT_HORIZON_Y: f32 = 0.0;
/// Squares per axis: `-384..=384` in steps of 64.
const SQUARES_PER_AXIS: usize = ((LAST - FIRST) / STEP + 1) as usize;
/// Triangle-list vertices of the grid.
pub(super) const GRID_VERTICES: usize = SQUARES_PER_AXIS * SQUARES_PER_AXIS * 6;

/// The grid at `y = 0`, as a triangle list.
pub(super) fn grid() -> Vec<SkyVertex> {
    let mut vertices = Vec::with_capacity(GRID_VERTICES);
    for x in (FIRST..=LAST).step_by(STEP as usize) {
        for z in (FIRST..=LAST).step_by(STEP as usize) {
            let (x0, x1) = (x as f32, (x + STEP) as f32);
            let (z0, z1) = (z as f32, (z + STEP) as f32);
            vertices.extend(quad([
                SkyVertex::plain([x0, 0.0, z0]),
                SkyVertex::plain([x1, 0.0, z0]),
                SkyVertex::plain([x1, 0.0, z1]),
                SkyVertex::plain([x0, 0.0, z1]),
            ]));
        }
    }
    vertices
}

/// Model of the sky dome: `glSkyList` at `y = +16`.
pub(super) fn sky_model() -> Mat4 {
    Mat4::from_translation(glam::Vec3::new(0.0, SKY_HEIGHT, 0.0))
}

/// Model of the coloured void plane: the `glSkyList2` call translated by
/// `-(d0 - 16)`, which lands it on the world horizon (`y = -d0` from the eye).
pub(super) fn void_model(below_horizon: f32) -> Mat4 {
    let lift = -(below_horizon - 16.0);
    Mat4::from_translation(glam::Vec3::new(0.0, VOID_HEIGHT + lift, 0.0))
}

/// Model of the black plane drawn while the eye is below the horizon: the
/// list translated by `+12`, so it hangs 4 blocks under the eye.
pub(super) fn black_model() -> Mat4 {
    Mat4::from_translation(glam::Vec3::new(0.0, VOID_HEIGHT + 12.0, 0.0))
}

/// The eye height above the world horizon (`d0` of the reference); a flat
/// world has its horizon at the bottom of the world.
pub(super) fn below_horizon(eye_y: f32, flat_world: bool) -> f32 {
    eye_y
        - if flat_world {
            FLAT_HORIZON_Y
        } else {
            HORIZON_Y
        }
}

/// The five faces (no bottom) of the black box around the eye, with its
/// floor at `y = -2`; [`box_model`] stretches it to the reference depth.
///
/// The reference draws a 2x2 prism from `y = -1` down to `-(d0 + 65)`.
pub(super) fn box_faces() -> Vec<SkyVertex> {
    let v = |x: f32, y: f32, z: f32| SkyVertex::plain([x, y, z]);
    let (top, floor) = (-1.0, -2.0);
    let faces = [
        [
            v(-1.0, floor, 1.0),
            v(1.0, floor, 1.0),
            v(1.0, top, 1.0),
            v(-1.0, top, 1.0),
        ],
        [
            v(-1.0, top, -1.0),
            v(1.0, top, -1.0),
            v(1.0, floor, -1.0),
            v(-1.0, floor, -1.0),
        ],
        [
            v(1.0, top, -1.0),
            v(1.0, top, 1.0),
            v(1.0, floor, 1.0),
            v(1.0, floor, -1.0),
        ],
        [
            v(-1.0, floor, -1.0),
            v(-1.0, floor, 1.0),
            v(-1.0, top, 1.0),
            v(-1.0, top, -1.0),
        ],
        [
            v(-1.0, top, -1.0),
            v(-1.0, top, 1.0),
            v(1.0, top, 1.0),
            v(1.0, top, -1.0),
        ],
    ];
    faces.into_iter().flat_map(quad).collect()
}

/// Stretches [`box_faces`] so the floor sits at `-(d0 + 65)` as in the
/// reference: `y' = -1 + (y + 1) * (d0 + 64)`.
pub(super) fn box_model(below_horizon: f32) -> Mat4 {
    Mat4::from_translation(glam::Vec3::new(0.0, -1.0, 0.0))
        * Mat4::from_scale(glam::Vec3::new(1.0, below_horizon + 64.0, 1.0))
        * Mat4::from_translation(glam::Vec3::new(0.0, 1.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_has_169_squares_of_two_triangles() {
        // 13 x 13 squares from -384 to 448, as the two nested reference loops.
        assert_eq!(SQUARES_PER_AXIS, 13);
        assert_eq!(grid().len(), 169 * 6);
        assert_eq!(grid().len(), GRID_VERTICES);
    }

    #[test]
    fn the_grid_spans_the_reference_extent_on_the_plane() {
        let vertices = grid();
        let min = vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::MAX, f32::min);
        let max = vertices
            .iter()
            .map(|v| v.position[2])
            .fold(f32::MIN, f32::max);
        assert_eq!((min, max), (-384.0, 448.0));
        assert!(vertices.iter().all(|v| v.position[1] == 0.0));
    }

    #[test]
    fn the_sky_hangs_above_the_eye_and_the_void_plane_lies_on_the_horizon() {
        let origin = glam::Vec3::ZERO;
        assert_eq!(sky_model().transform_point3(origin).y, 16.0);
        // Eye at y = 80: d0 = 17, the horizon (y = 63) is 17 below the eye.
        let d0 = below_horizon(80.0, false);
        assert_eq!(void_model(d0).transform_point3(origin).y, -17.0);
        // Eye at y = 40: the same plane is 23 above... d0 = -23.
        let d0 = below_horizon(40.0, false);
        assert_eq!(void_model(d0).transform_point3(origin).y, 23.0);
        assert_eq!(black_model().transform_point3(origin).y, -4.0);
    }

    #[test]
    fn a_flat_world_has_its_horizon_at_the_bottom_not_at_63() {
        // The bug: a player on a superflat ground (y = 5) was taken to be 58
        // blocks below the horizon, so the void planes hung above the world.
        assert_eq!(below_horizon(5.0, false), -58.0);
        assert_eq!(below_horizon(5.0, true), 5.0);
        // On the flat ground the eye is above the horizon, so the black plane
        // and box (drawn only below it) are left out.
        assert!(below_horizon(5.0, true) > 0.0);
        // The horizon plane lies 5 blocks below the eye, `-d0`.
        let plane = void_model(below_horizon(5.0, true)).transform_point3(glam::Vec3::ZERO);
        assert_eq!(plane.y, -5.0);
    }

    #[test]
    fn the_box_floor_reaches_the_reference_depth() {
        assert_eq!(box_faces().len(), 5 * 6);
        let d0 = -10.0;
        let floor = box_model(d0).transform_point3(glam::Vec3::new(0.0, -2.0, 0.0));
        // `f19 = -(d0 + 65)`.
        assert_eq!(floor.y, -(d0 + 65.0));
        let top = box_model(d0).transform_point3(glam::Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(top.y, -1.0);
    }
}
