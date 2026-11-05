// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the title-screen panorama pass with six solid
//! synthetic colour faces (no official assets).
//!
//! Run with `cargo test -p umicraft-render --test offscreen_panorama -- --ignored`
//! on a machine with a wgpu adapter.

use umicraft_render::{Camera, CloudMode, Image, Overlay, Renderer, SkyImage};
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

/// Renders the sharp (unblurred) panorama at `yaw_degrees` / `pitch_degrees`.
fn frame(yaw_degrees: f32, pitch_degrees: f32, shown: bool) -> Image {
    render(yaw_degrees, pitch_degrees, shown, false)
}

/// Renders the panorama with or without the vanilla blur.
fn render(yaw_degrees: f32, pitch_degrees: f32, shown: bool, blur: bool) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_panorama_blur(blur);
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

#[test]
#[ignore = "requires a GPU adapter"]
fn the_world_passes_do_not_show_through_the_panorama() {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_panorama_blur(false);
    renderer.set_cloud_mode(CloudMode::Fancy);
    let textures = renderer.upload_panorama(&faces()).expect("valid faces");
    renderer.set_panorama(Some(PanoramaView {
        textures,
        rotation: PanoramaRotation {
            yaw_degrees: -20.0,
            pitch_degrees: 20.0,
        },
    }));
    // Looking up from below the clouds: without the panorama they fill the view.
    let mut camera = Camera::new(glam::Vec3::new(0.0, 100.0, 0.0), 0.0, 0.8);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    let image = renderer.read_pixels().expect("read pixels");
    let foreign = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| !COLOURS.contains(&pixel(&image, x, y)))
        .count();
    // Only the seams between faces, where bilinear filtering blends, differ.
    assert!(foreign < 200, "{foreign} pixels are not a face colour");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_blur_softens_the_seams_without_opening_gaps() {
    // 45 degrees puts the seam of two faces through the middle.
    let sharp = render(-45.0, 0.0, true, false);
    let blurred = render(-45.0, 0.0, true, true);
    let differing = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&sharp, x, y) != pixel(&blurred, x, y))
        .count();
    assert!(differing > 100, "{differing} pixels changed");
    // The blur mixes face colours near the seam; it never leaves black
    // pixels (a gap in the cube or a missed pass).
    let dark = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&blurred, x, y).iter().all(|&c| c < 40))
        .count();
    assert_eq!(dark, 0, "{dark} dark pixels");
    // Far from a seam the colour is the face colour.
    let corner = pixel(&blurred, 6, HEIGHT / 2);
    assert!(
        corner
            .iter()
            .zip(COLOURS[0])
            .all(|(a, b)| a.abs_diff(b) < 25),
        "{corner:?}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_blurred_centre_still_shows_the_face_the_rotation_points_at() {
    let ahead = render(0.0, 0.0, true, true);
    let centre = pixel(&ahead, WIDTH / 2, HEIGHT / 2);
    assert!(
        centre
            .iter()
            .zip(COLOURS[0])
            .all(|(a, b)| a.abs_diff(b) < 25),
        "{centre:?}"
    );
}

#[test]
#[ignore = "requires a GPU adapter; prints timings for docs/perf-log.md"]
fn the_blur_cost_per_frame() {
    let time = |blur: bool| {
        let mut renderer = Renderer::new_offscreen(1280, 720).expect("offscreen renderer");
        renderer.set_panorama_blur(blur);
        let textures = renderer.upload_panorama(&faces()).expect("valid faces");
        let camera = Camera::new(glam::Vec3::new(0.0, 100.0, 0.0), 0.0, 0.0);
        let overlay = Overlay::new(1280, 720);
        let view = PanoramaView {
            textures,
            rotation: PanoramaRotation {
                yaw_degrees: -30.0,
                pitch_degrees: 20.0,
            },
        };
        let start = std::time::Instant::now();
        for _ in 0..60 {
            renderer.set_panorama(Some(view));
            renderer.render(&camera, &overlay).expect("render");
            renderer.read_pixels().expect("read pixels");
        }
        start.elapsed().as_secs_f64() * 1000.0 / 60.0
    };
    let (sharp, blurred) = (time(false), time(true));
    println!("panorama frame at 1280x720 (render + readback): sharp {sharp:.2} ms, blurred {blurred:.2} ms");
}
