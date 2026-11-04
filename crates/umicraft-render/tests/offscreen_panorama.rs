// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the title-screen panorama pass with six solid
//! synthetic colour faces (no official assets).
//!
//! Run with `cargo test -p umicraft-render --test offscreen_panorama -- --ignored`
//! on a machine with a wgpu adapter.

use umicraft_render::{Camera, Image, Overlay, Renderer, SkyImage};
use umicraft_ui::menus::{PanoramaRotation, PanoramaView};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;
const FACE_SIZE: u32 = 8;

/// One distinct, strongly separated colour per face.
const COLOURS: [[u8; 3]; 6] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
    [255, 0, 255],
];

fn faces() -> [SkyImage; 6] {
    COLOURS.map(|[r, g, b]| SkyImage {
        width: FACE_SIZE,
        height: FACE_SIZE,
        rgba: [r, g, b, 255].repeat((FACE_SIZE * FACE_SIZE) as usize),
    })
}

/// Renders the panorama at `yaw_degrees` / `pitch_degrees`.
fn frame(yaw_degrees: f32, pitch_degrees: f32, shown: bool) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    let textures = renderer.upload_panorama(&faces()).expect("valid faces");
    renderer.set_panorama(shown.then_some(PanoramaView {
        textures,
        rotation: PanoramaRotation {
            yaw_degrees,
            pitch_degrees,
        },
    }));
    let mut camera = Camera::new(glam::Vec3::new(0.0, 100.0, 0.0), 0.0, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("read pixels")
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 3] {
    let at = ((y * image.width + x) * 4) as usize;
    [image.rgba[at], image.rgba[at + 1], image.rgba[at + 2]]
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_centre_shows_the_face_the_rotation_points_at() {
    let ahead = frame(0.0, 0.0, true);
    assert_eq!(pixel(&ahead, WIDTH / 2, HEIGHT / 2), COLOURS[0]);
    let quarter = frame(-90.0, 0.0, true);
    assert_eq!(pixel(&quarter, WIDTH / 2, HEIGHT / 2), COLOURS[1]);
    let up = frame(0.0, 90.0, true);
    assert_eq!(pixel(&up, WIDTH / 2, HEIGHT / 2), COLOURS[5]);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_rotation_between_two_faces_shows_both() {
    // 45 degrees: the seam is at the centre and both neighbours are on screen.
    let image = frame(-45.0, 0.0, true);
    let left = pixel(&image, 8, HEIGHT / 2);
    let right = pixel(&image, WIDTH - 8, HEIGHT / 2);
    let sides = [left, right];
    assert!(
        sides.contains(&COLOURS[0]) && sides.contains(&COLOURS[1]),
        "{sides:?}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn without_a_view_nothing_is_drawn() {
    let image = frame(0.0, 0.0, false);
    for colour in COLOURS {
        assert_ne!(pixel(&image, WIDTH / 2, HEIGHT / 2), colour);
    }
}
