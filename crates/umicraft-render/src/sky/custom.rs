// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pack-defined sky layers (OptiFine-compatible custom sky): the textured
//! cube, its orientation and the blend-mode mapping.
//!
//! Behaviour source: `CustomSkyLayer.render` / `renderSide` and `Blender`
//! of the OptiFine reference (ADR-0088, docs/sources.md). A layer is a cube
//! of half-size 100 around the eye whose picture is a 3 x 2 sheet of faces.
//! The reference draws the cube with a chain of matrix rotations; they are
//! baked into the six faces here, so every layer shares one 36-vertex mesh and
//! only its model matrix (base turn, then the pack's rotation) changes.

use glam::{Mat4, Quat, Vec3};
use umicraft_packext::sky::Blend as PackBlend;

use super::plan::Blend;
use super::vertex::{quad, SkyVertex};

/// Vertices of the cube (six quads of two triangles).
pub(super) const CUBE_VERTICES: usize = 36;
/// Half the cube side, in blocks.
const HALF: f32 = 100.0;
/// GL's default alpha function of `GlStateManager` is `GREATER 0.1`; the
/// replace mode relies on it to cut the transparent texels.
pub(super) const ALPHA_TEST: f32 = 0.1;

/// The face quad of `side`: its sheet cell (`u = side % 3 / 3`,
/// `v = side / 3 / 2`) on the reference's `-Y` square.
fn face(side: u32) -> [([f32; 3], [f32; 2]); 4] {
    let u = (side % 3) as f32 / 3.0;
    let v = (side / 3) as f32 / 2.0;
    let (du, dv) = (1.0 / 3.0, 0.5);
    [
        ([-HALF, -HALF, -HALF], [u, v]),
        ([-HALF, -HALF, HALF], [u, v + dv]),
        ([HALF, -HALF, HALF], [u + du, v + dv]),
        ([HALF, -HALF, -HALF], [u + du, v]),
    ]
}

/// The matrix each side is turned by, in the reference's draw order
/// (sides 4, 1, 0, 5, 2, 3). The last three accumulate quarter turns about Z.
fn side_turns() -> [(u32, Mat4); 6] {
    let quarter = std::f32::consts::FRAC_PI_2;
    [
        (4, Mat4::IDENTITY),
        (1, Mat4::from_rotation_x(quarter)),
        (0, Mat4::from_rotation_x(-quarter)),
        (5, Mat4::from_rotation_z(quarter)),
        (2, Mat4::from_rotation_z(2.0 * quarter)),
        (3, Mat4::from_rotation_z(3.0 * quarter)),
    ]
}

/// The shared cube, with each face's turn applied.
pub(super) fn cube() -> Vec<SkyVertex> {
    side_turns()
        .into_iter()
        .flat_map(|(side, turn)| {
            quad(face(side).map(|(position, uv)| {
                let turned = turn.transform_point3(Vec3::from(position));
                SkyVertex::textured(turned.to_array(), uv)
            }))
        })
        .collect()
}

/// The model matrix of a layer: the sky's `-90` degrees about Y, the pack's
/// rotation (`degrees` about `axis`, already in the reference's permuted
/// frame), then the base `90` about X and `-90` about Z.
pub(super) fn model(rotation: Option<f32>, axis: [f32; 3]) -> Mat4 {
    let quarter = std::f32::consts::FRAC_PI_2;
    let pack = match (rotation, Vec3::from(axis).try_normalize()) {
        (Some(degrees), Some(axis)) => {
            Mat4::from_quat(Quat::from_axis_angle(axis, degrees.to_radians()))
        }
        _ => Mat4::IDENTITY,
    };
    Mat4::from_rotation_y(-quarter)
        * pack
        * Mat4::from_rotation_x(quarter)
        * Mat4::from_rotation_z(-quarter)
}

/// The pipeline blend of a pack blend mode, and whether the draw alpha-tests
/// (only `replace`, which switches blending off and the alpha test on).
pub(super) fn blend(mode: PackBlend) -> (Blend, bool) {
    match mode {
        PackBlend::Alpha => (Blend::Alpha, false),
        PackBlend::Add => (Blend::Additive, false),
        PackBlend::Subtract => (Blend::Subtract, false),
        PackBlend::Multiply => (Blend::Multiply, false),
        PackBlend::Dodge => (Blend::Dodge, false),
        PackBlend::Burn => (Blend::Burn, false),
        PackBlend::Screen => (Blend::Screen, false),
        PackBlend::Overlay => (Blend::Overlay, false),
        PackBlend::Replace => (Blend::Opaque, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_has_six_faces_at_distance_100_with_distinct_cells() {
        let vertices = cube();
        assert_eq!(vertices.len(), CUBE_VERTICES);
        for axis in 0..3 {
            for sign in [-HALF, HALF] {
                let on_face = vertices
                    .iter()
                    .filter(|v| (v.position[axis] - sign).abs() < 1e-3)
                    .count();
                assert!(on_face >= 6, "axis {axis} sign {sign}");
            }
        }
        // Each face samples one sixth of the sheet.
        let mut cells: Vec<(u32, u32)> = vertices
            .chunks(6)
            .map(|f| {
                let u = f.iter().map(|v| v.uv[0]).fold(f32::MAX, f32::min);
                let v = f.iter().map(|v| v.uv[1]).fold(f32::MAX, f32::min);
                ((u * 3.0).round() as u32, (v * 2.0).round() as u32)
            })
            .collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), 6);
        assert!(vertices
            .iter()
            .all(|v| (0.0..=1.0).contains(&v.uv[0]) && (0.0..=1.0).contains(&v.uv[1])));
    }

    #[test]
    fn side_four_is_the_floor_face_before_the_base_turn() {
        // The reference's first face is the -Y square, drawn without a turn.
        let first = &cube()[..6];
        assert!(first.iter().all(|v| (v.position[1] + HALF).abs() < 1e-3));
        assert_eq!(first[0].uv, [1.0 / 3.0, 0.5]);
    }

    #[test]
    fn the_base_turn_maps_the_reference_floor_face() {
        // rotate(90, X) then rotate(-90, Z) on the -Y face: the point (0,-1,0)
        // goes to (1,0,0) after the Y turn is added.
        let m = model(None, [0.0, 0.0, -1.0]);
        let p = m.transform_point3(Vec3::NEG_Y);
        let base = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
            * Mat4::from_rotation_z(-std::f32::consts::FRAC_PI_2);
        let expected = Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
            .transform_point3(base.transform_point3(Vec3::NEG_Y));
        assert!((p - expected).length() < 1e-5);
    }

    #[test]
    fn a_rotation_turns_about_the_given_axis_between_the_fixed_turns() {
        let fixed = model(None, [0.0, 1.0, 0.0]);
        let turned = model(Some(90.0), [0.0, 1.0, 0.0]);
        let quarter = std::f32::consts::FRAC_PI_2;
        let expected = Mat4::from_rotation_y(-quarter)
            * Mat4::from_rotation_y(quarter)
            * Mat4::from_rotation_x(quarter)
            * Mat4::from_rotation_z(-quarter);
        assert!(turned.abs_diff_eq(expected, 1e-5));
        assert!(!turned.abs_diff_eq(fixed, 1e-3));
    }

    #[test]
    fn an_unnormalised_axis_is_normalised() {
        let a = model(Some(40.0), [0.0, 0.0, -1.0]);
        let b = model(Some(40.0), [0.0, 0.0, -0.5]);
        assert!(a.abs_diff_eq(b, 1e-5));
    }

    #[test]
    fn every_pack_blend_has_its_own_pipeline_and_only_replace_alpha_tests() {
        let mapped = PackBlend::ALL.map(blend);
        for (mode, (pipeline, tested)) in PackBlend::ALL.into_iter().zip(mapped) {
            assert_eq!(tested, mode == PackBlend::Replace);
            assert_eq!(
                mapped
                    .iter()
                    .filter(|(other, _)| *other == pipeline)
                    .count(),
                1
            );
        }
        assert_eq!(blend(PackBlend::Add).0, Blend::Additive);
    }
}
