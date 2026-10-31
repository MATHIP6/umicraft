// SPDX-License-Identifier: MIT OR Apache-2.0

//! The End sky: a cube of `environment/end_sky.png` (`renderSkyEnd`).
//!
//! The reference draws one 200 x 200 face six times, turned by
//! `rotate(90|-90|180, x)` and `rotate(90|-90, z)`, with the texture repeated
//! 16 times and the vertex colour `(40, 40, 40)`. The rotations are baked into
//! one 36-vertex mesh because the cube never moves.

use glam::{Mat4, Vec3};

use super::vertex::{quad, SkyVertex};

/// Half the face side.
const HALF: f32 = 100.0;
/// Repeats of the texture across a face.
const REPEAT: f32 = 16.0;
/// `color(40, 40, 40, 255)` of the reference.
const SHADE: f32 = 40.0 / 255.0;

/// The six face rotations in the reference's order (`i == 0` is the identity).
fn face_rotations() -> [Mat4; 6] {
    let quarter = std::f32::consts::FRAC_PI_2;
    [
        Mat4::IDENTITY,
        Mat4::from_rotation_x(quarter),
        Mat4::from_rotation_x(-quarter),
        Mat4::from_rotation_x(std::f32::consts::PI),
        Mat4::from_rotation_z(quarter),
        Mat4::from_rotation_z(-quarter),
    ]
}

/// The cube as a triangle list around the eye.
pub(super) fn cube() -> Vec<SkyVertex> {
    let colour = [SHADE, SHADE, SHADE, 1.0];
    let face = [
        ([-HALF, -HALF, -HALF], [0.0, 0.0]),
        ([-HALF, -HALF, HALF], [0.0, REPEAT]),
        ([HALF, -HALF, HALF], [REPEAT, REPEAT]),
        ([HALF, -HALF, -HALF], [REPEAT, 0.0]),
    ];
    face_rotations()
        .into_iter()
        .flat_map(|rotation| {
            quad(face.map(|(position, uv)| {
                let turned = rotation.transform_point3(Vec3::from(position));
                SkyVertex::coloured(turned.to_array(), uv, colour)
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_has_six_faces_around_the_eye() {
        let vertices = cube();
        assert_eq!(vertices.len(), 6 * 6);
        // Each axis direction has one face at distance 100.
        for axis in 0..3 {
            for sign in [-100.0, 100.0] {
                let face = vertices
                    .iter()
                    .filter(|v| (v.position[axis] - sign).abs() < 1e-3)
                    .count();
                assert!(face >= 6, "axis {axis} sign {sign}: {face}");
            }
        }
    }

    #[test]
    fn the_faces_are_shaded_to_40_and_repeat_sixteen_times() {
        let vertices = cube();
        assert!(vertices
            .iter()
            .all(|v| v.color == [40.0 / 255.0, 40.0 / 255.0, 40.0 / 255.0, 1.0]));
        let max_u = vertices.iter().map(|v| v.uv[0]).fold(0.0, f32::max);
        assert_eq!(max_u, 16.0);
    }
}
