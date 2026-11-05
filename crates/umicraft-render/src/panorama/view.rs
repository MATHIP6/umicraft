// SPDX-License-Identifier: MIT OR Apache-2.0

//! The panorama camera and the cube geometry, as plain maths.
//!
//! Source: `GuiMainMenu.renderSkybox` (model-view `rotate(180, x)`,
//! `rotate(90, z)`, `rotate(pitch, x)`, `rotate(yaw, y)`, then the face turn)
//! and `rotateAndBlurSkybox` (the cube is drawn into a 256 x 256 viewport at
//! aspect 1 and only its central 120 / 256 is stretched over the screen).

use glam::{Mat4, Vec3, Vec4};
use umicraft_ui::menus::panorama::{CLIP_PLANES, FACE_TURNS_DEGREES, FIELD_OF_VIEW_DEGREES};
use umicraft_ui::menus::PanoramaRotation;

/// Edge of the framebuffer square vanilla renders the cube into.
const CAPTURE_SIZE: f32 = 256.0;
/// Edge of the central part of that square shown on screen
/// (`120 / max(width, height)` in `rotateAndBlurSkybox`).
const VISIBLE_SIZE: f32 = 120.0;

/// One cube vertex: position and texture coordinate.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct FaceVertex {
    pub(super) position: [f32; 3],
    pub(super) uv: [f32; 2],
}

impl FaceVertex {
    /// The vertex attributes of `panorama.wgsl`.
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
}

/// Vertices per face: two triangles.
pub(super) const FACE_VERTICES: u32 = 6;

/// The six faces as triangle lists, in file order.
pub(super) fn cube_vertices() -> Vec<FaceVertex> {
    // The quad of the reference, at z = 1, as (x, y, u, v).
    const CORNERS: [(f32, f32, f32, f32); 4] = [
        (-1.0, -1.0, 0.0, 0.0),
        (1.0, -1.0, 1.0, 0.0),
        (1.0, 1.0, 1.0, 1.0),
        (-1.0, 1.0, 0.0, 1.0),
    ];
    const TRIANGLES: [usize; 6] = [0, 1, 2, 0, 2, 3];
    FACE_TURNS_DEGREES
        .iter()
        .flat_map(|&(yaw, pitch)| {
            let turn =
                Mat4::from_rotation_y(yaw.to_radians()) * Mat4::from_rotation_x(pitch.to_radians());
            TRIANGLES.iter().map(move |&corner| {
                let (x, y, u, v) = CORNERS[corner];
                FaceVertex {
                    position: turn.transform_point3(Vec3::new(x, y, 1.0)).to_array(),
                    uv: [u, v],
                }
            })
        })
        .collect()
}

/// How much the screen magnifies the square capture along its two axes.
///
/// `rotateAndBlurSkybox` shows `120 / max(width, height)` of the capture per
/// screen pixel, so the larger side sees the central 120 / 256 of it.
fn magnification(width: f32, height: f32) -> (f32, f32) {
    let larger = width.max(height);
    let per_pixel = CAPTURE_SIZE / (2.0 * VISIBLE_SIZE);
    (per_pixel * larger / width, per_pixel * larger / height)
}

/// How much the screen magnifies the square capture along its two axes
/// (`kx`, `ky`) for a `width` x `height` target.
pub(super) fn fit_scale(width: u32, height: u32) -> [f32; 2] {
    let (kx, ky) = magnification(width.max(1) as f32, height.max(1) as f32);
    [kx, ky]
}

/// The matrix from cube space to clip space for a `width` x `height` target
/// and the rotation of this frame. Depth is wgpu's 0..1.
///
/// The reference stretches its capture over the screen with the texture
/// coordinates transposed: screen right is capture down and screen up is
/// capture right, a quarter turn the matrix `fit` reproduces.
pub(super) fn clip_from_cube(width: u32, height: u32, rotation: PanoramaRotation) -> Mat4 {
    let [kx, ky] = fit_scale(width, height);
    let fit = Mat4::from_cols(
        Vec4::new(0.0, ky, 0.0, 0.0),
        Vec4::new(-kx, 0.0, 0.0, 0.0),
        Vec4::Z,
        Vec4::W,
    );
    fit * capture_from_cube(rotation, [0.0, 0.0])
}

/// The matrix from cube space to the clip space of the square 256 x 256
/// capture. `jitter` is the sub-pixel translation of one of the 64 passes of
/// `drawPanorama`, applied after the base rotations and before the pitch and
/// the yaw.
pub(super) fn capture_from_cube(rotation: PanoramaRotation, jitter: [f32; 2]) -> Mat4 {
    let capture = glam::camera::rh::proj::directx::perspective(
        FIELD_OF_VIEW_DEGREES.to_radians(),
        1.0,
        CLIP_PLANES.0,
        CLIP_PLANES.1,
    );
    let model_view = Mat4::from_rotation_x(180f32.to_radians())
        * Mat4::from_rotation_z(90f32.to_radians())
        * Mat4::from_translation(Vec3::new(jitter[0], jitter[1], 0.0))
        * Mat4::from_rotation_x(rotation.pitch_degrees.to_radians())
        * Mat4::from_rotation_y(rotation.yaw_degrees.to_radians());
    capture * model_view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotation(yaw_degrees: f32, pitch_degrees: f32) -> PanoramaRotation {
        PanoramaRotation {
            yaw_degrees,
            pitch_degrees,
        }
    }

    /// The point of the unit cube at the centre of face `index`, in NDC.
    fn centre_ndc(index: usize, rotation: PanoramaRotation) -> (f32, f32, f32) {
        let vertices = cube_vertices();
        let face = &vertices[index * 6..index * 6 + 6];
        let centre = face.iter().map(|v| Vec3::from(v.position)).sum::<Vec3>() / 6.0;
        let clip = clip_from_cube(160, 90, rotation) * centre.extend(1.0);
        (clip.x / clip.w, clip.y / clip.w, clip.w)
    }

    #[test]
    fn the_first_face_is_straight_ahead_at_rest() {
        let (x, y, w) = centre_ndc(0, rotation(0.0, 0.0));
        assert!(w > 0.0 && x.abs() < 0.2 && y.abs() < 0.2, "{x} {y} {w}");
    }

    #[test]
    fn a_quarter_turn_brings_the_second_face_ahead() {
        let (x, y, w) = centre_ndc(1, rotation(-90.0, 0.0));
        assert!(w > 0.0 && x.abs() < 0.2 && y.abs() < 0.2, "{x} {y} {w}");
        let (_, _, opposite) = centre_ndc(3, rotation(-90.0, 0.0));
        assert!(opposite < 0.0, "the opposite face is behind: {opposite}");
    }

    #[test]
    fn looking_up_brings_the_fifth_face_ahead() {
        let (x, y, w) = centre_ndc(5, rotation(0.0, 90.0));
        assert!(w > 0.0 && x.abs() < 0.2 && y.abs() < 0.2, "{x} {y} {w}");
    }

    #[test]
    fn the_larger_side_gets_the_vanilla_central_crop() {
        let (kx, ky) = magnification(1600.0, 900.0);
        assert!((kx - 256.0 / 240.0).abs() < 1e-5);
        assert!((ky - kx * 1600.0 / 900.0).abs() < 1e-4);
    }

    #[test]
    fn the_jitter_moves_the_picture_by_less_than_a_texel() {
        let point = Vec3::new(0.3, 0.2, 1.0).extend(1.0);
        let at = |jitter: [f32; 2]| {
            let clip = capture_from_cube(rotation(0.0, 0.0), jitter) * point;
            (clip.x / clip.w, clip.y / clip.w)
        };
        let (x0, y0) = at([0.0, 0.0]);
        let (x1, y1) = at([-1.0 / 128.0, 1.0 / 128.0]);
        // One texel of the 256 px capture is 2 / 256 of clip space.
        assert!((x1 - x0).abs() + (y1 - y0).abs() < 2.0 / 256.0 * 2.0);
        assert!((x1 - x0).abs() + (y1 - y0).abs() > 0.0);
    }

    #[test]
    fn the_cube_has_six_faces_of_six_vertices() {
        assert_eq!(cube_vertices().len(), 6 * FACE_VERTICES as usize);
    }
}
