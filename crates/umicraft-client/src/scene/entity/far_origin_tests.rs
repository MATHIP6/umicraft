// SPDX-License-Identifier: MIT OR Apache-2.0

//! F2-1b regression: an entity far from the world origin is drawn where it
//! is, through the real path (frame data, interpolation relative to the
//! render origin, local camera of the entity pass).
//!
//! The same scene is rendered near the origin and at x = z = 1 000 000 and the
//! pixels around the entity are compared: they must agree. Before F2-1b the
//! 6 cm grain of an absolute f32 position showed as a different, jittery
//! picture. GPU test: `cargo test -p umicraft-client --lib far_origin -- --ignored`.

use glam::DVec3;
use umicraft_game::entity::{EntityId, EntityKind, EntityRecord};
use umicraft_render::{Camera, CloudMode, Image, Overlay, RenderEnvironment, Renderer};

use super::EntityTextures;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 192;

/// Renders a pig 6 blocks in front of the camera, with the scene shifted by
/// `base` blocks on x and z. `step` moves the pig by that many blocks.
fn pig_scene(base: f64, step: f64) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_cloud_mode(CloudMode::Off);
    renderer.set_environment(RenderEnvironment::new(0, 6_000, 0.0, 8));
    let eye = DVec3::new(base + 0.5, 65.0, base - 6.0);
    let mut camera = Camera::new(glam::Vec3::ZERO, std::f32::consts::PI, 0.0);
    camera.set_eye(eye);
    camera.set_aspect(WIDTH, HEIGHT);

    let mut record = EntityRecord::new(
        EntityId(1),
        EntityKind::Pig,
        DVec3::new(base + 0.5 + step, 64.0, base + 0.5),
        0.0,
        0.0,
    );
    record.previous_position = record.position;
    let snapshot = record.frame_data().interpolate(1.0, camera.render_origin());
    let textures = EntityTextures::load(&renderer);
    textures.set_camera_position(camera.relative_eye());
    textures.mesh_and_upload(&[snapshot], &mut renderer);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("readback")
}

/// How many pixels differ by more than a few levels.
fn differing(a: &Image, b: &Image) -> usize {
    a.rgba
        .chunks_exact(4)
        .zip(b.rgba.chunks_exact(4))
        .filter(|(x, y)| (0..3).any(|i| x[i].abs_diff(y[i]) > 12))
        .count()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn an_entity_one_million_blocks_out_looks_like_one_at_the_origin() {
    let near = pig_scene(0.0, 0.0);
    let far = pig_scene(1_000_000.0, 0.0);
    let pig_pixels = differing(&near, &empty_scene(0.0));
    assert!(pig_pixels > 200, "the pig is on screen: {pig_pixels}");
    let mismatch = differing(&near, &far);
    assert!(
        mismatch * 50 < pig_pixels,
        "{mismatch} pixels differ out of {pig_pixels}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_five_centimetre_step_moves_the_entity_far_from_the_origin() {
    // An absolute f32 position at x = 1 000 000 has a 6 cm grain, so the
    // step vanished.
    let still = pig_scene(1_000_000.0, 0.0);
    let moved = pig_scene(1_000_000.0, 0.05);
    assert!(
        differing(&still, &moved) > 0,
        "a 5 cm step changes the frame"
    );
}

/// The scene without the pig, to count the pixels it covers.
fn empty_scene(base: f64) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_cloud_mode(CloudMode::Off);
    renderer.set_environment(RenderEnvironment::new(0, 6_000, 0.0, 8));
    let mut camera = Camera::new(glam::Vec3::ZERO, std::f32::consts::PI, 0.0);
    camera.set_eye(DVec3::new(base + 0.5, 65.0, base - 6.0));
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("readback")
}
