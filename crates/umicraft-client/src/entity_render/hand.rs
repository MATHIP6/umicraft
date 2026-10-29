// SPDX-License-Identifier: MIT OR Apache-2.0

//! The first-person arm mesh.

use glam::{Mat4, Vec3};
use umicraft_game::hand::HandPose;
use umicraft_model::entity::HumanoidModel;
use umicraft_render::EntityVertex;

/// The vertices of the first-person arm in camera space.
///
/// Empty when the pose shows no arm. `bob` is the view-bobbing transform of
/// the frame (MCP applies it to the hand as well).
#[must_use]
pub fn hand_vertices(pose: &HandPose, bob: Mat4, is_slim: bool) -> Vec<EntityVertex> {
    if !pose.shows_arm {
        return Vec::new();
    }
    let to_camera = bob * pose.arm_matrix();
    HumanoidModel::first_person_arm(is_slim)
        .into_iter()
        .map(|vertex| EntityVertex {
            position: to_camera
                .transform_point3(Vec3::from(vertex.position))
                .to_array(),
            normal: to_camera
                .transform_vector3(Vec3::from(vertex.normal))
                .normalize_or_zero()
                .to_array(),
            uv: vertex.uv,
            // Full sky light, no block light: the hand darkens at night.
            light: HAND_LIGHT,
        })
        .collect()
}

/// The packed light of the hand (bits 4..=7 are the sky light).
const HAND_LIGHT: u32 = 0x0000_00F0;

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_render::Renderer;

    fn shown() -> HandPose {
        HandPose {
            shows_arm: true,
            ..HandPose::default()
        }
    }

    #[test]
    fn a_hand_that_shows_no_arm_has_no_vertices() {
        assert!(hand_vertices(&HandPose::default(), Mat4::IDENTITY, false).is_empty());
    }

    #[test]
    fn the_resting_arm_is_in_the_lower_right_in_front_of_the_camera() {
        let vertices = hand_vertices(&shown(), Mat4::IDENTITY, false);
        assert_eq!(vertices.len(), 72);
        let count = vertices.len() as f32;
        let centre = vertices
            .iter()
            .fold(Vec3::ZERO, |sum, v| sum + Vec3::from(v.position))
            / count;
        assert!(centre.x > 0.1, "to the right: {centre:?}");
        assert!(centre.y < 0.0, "below the eyes: {centre:?}");
        assert!(centre.z < -0.3, "in front of the camera: {centre:?}");
        for vertex in &vertices {
            assert!(vertex.position[2] < 0.0, "behind the camera: {vertex:?}");
            assert_eq!(vertex.light, HAND_LIGHT);
        }
    }

    #[test]
    fn the_view_bobbing_moves_the_hand_with_the_camera() {
        let still = hand_vertices(&shown(), Mat4::IDENTITY, false);
        let bobbed = hand_vertices(
            &shown(),
            Mat4::from_translation(Vec3::new(0.0, -0.05, 0.0)),
            false,
        );
        assert!((bobbed[0].position[1] - still[0].position[1] + 0.05).abs() < 1e-5);
    }

    /// The real arm, rendered with a red skin on a software adapter, sits in
    /// the lower right of the frame and leaves the upper left alone.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn the_arm_is_drawn_in_the_lower_right_of_the_frame() {
        use umicraft_render::{Camera, Overlay};

        let (width, height) = (320, 180);
        let mut renderer = Renderer::new_offscreen(width, height).expect("offscreen renderer");
        let skin = renderer.create_entity_texture(&[255, 0, 0, 255], 1, 1);
        let vertices = hand_vertices(&shown(), Mat4::IDENTITY, false);
        renderer.set_hand(&vertices, skin.bind_group());

        let mut camera = Camera::default();
        camera.set_aspect(width, height);
        renderer
            .render(&camera, &Overlay::new(width, height))
            .expect("render the arm");
        let image = renderer.read_pixels().expect("pixels");

        let red = |x: u32, y: u32| {
            let at = ((y * width + x) * 4) as usize;
            image.rgba[at] > 60 && image.rgba[at + 1] < image.rgba[at] / 3
        };
        let count = |x0: u32, y0: u32, x1: u32, y1: u32| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| red(x, y))
                .count()
        };
        let lower_right = count(width / 2, height / 2, width, height);
        let upper_left = count(0, 0, width / 2, height / 2);
        assert!(lower_right > 400, "the arm must show: {lower_right} pixels");
        assert_eq!(upper_left, 0, "and stay out of the upper left");
    }
}
