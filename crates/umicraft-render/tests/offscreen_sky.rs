// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the sky geometry pass: the dome, the stars, the
//! Nether clear colour and the End cube. The procedural fallback images are
//! used, so no official assets are required.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_sky -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_render::{Camera, CloudMode, Image, Overlay, RenderEnvironment, Renderer, SkyState};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;

/// Renders one frame looking `pitch` radians up from a high vantage point.
fn frame(environment: RenderEnvironment, pitch: f32) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(environment);
    let mut camera = Camera::new(Vec3::new(0.0, 100.0, 0.0), 0.0, pitch);
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

/// Pixels whose weakest channel exceeds `threshold`: white-ish ones only.
fn bright_pixels(image: &Image, threshold: u8) -> usize {
    image
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0].min(p[1]).min(p[2]) > threshold)
        .count()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_noon_zenith_is_the_sky_blue_and_the_view_is_not_flat() {
    let image = frame(RenderEnvironment::default(), 0.35);
    // Pitched 20 degrees up the sun (at the zenith at noon) is out of view.
    let [r, _, b] = pixel(&image, WIDTH / 2, 4);
    assert!(b > r, "the zenith leans blue: {r} {b}");
    let horizon = frame(RenderEnvironment::default(), 0.0);
    assert_ne!(
        pixel(&horizon, WIDTH / 2, 4),
        pixel(&horizon, WIDTH / 2, HEIGHT - 4)
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_night_sky_shows_stars_that_the_day_does_not() {
    let night = RenderEnvironment::new(0, 18_000, 0.0, 8);
    let day = RenderEnvironment::default();
    // Looking up at midnight the moon is below the horizon: any bright texel
    // is a star. At noon the same view shows none (the sun is at the zenith,
    // so look 20 degrees up only).
    let stars = bright_pixels(&frame(night, 1.0), 60);
    assert!(stars >= 3, "stars at night: {stars}");
    assert_eq!(bright_pixels(&frame(day, 0.35), 250), 0);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_nether_is_the_plain_fog_colour_and_the_end_draws_its_cube() {
    let nether = RenderEnvironment::new(-1, 0, 0.0, 8);
    let image = frame(nether, 0.5);
    // The target is not sRGB: the clear colour reads back as stored.
    let encode = |c: f32| (c * 255.0).round() as i32;
    let fog = SkyState::at(nether).fog.color.map(encode);
    for (x, y) in [(2, 2), (WIDTH / 2, HEIGHT / 2), (WIDTH - 3, HEIGHT - 3)] {
        let got = pixel(&image, x, y).map(i32::from);
        for channel in 0..3 {
            assert!(
                (got[channel] - fog[channel]).abs() <= 6,
                "{got:?} vs {fog:?}"
            );
        }
    }
    let end = frame(RenderEnvironment::new(1, 0, 0.0, 8), 0.5);
    assert_ne!(
        pixel(&end, 2, 2),
        pixel(&image, 2, 2),
        "the cube covers the fog"
    );
}

/// Renders a frame from `x`, below the cloud layer, looking straight up.
fn cloud_frame(mode: CloudMode, x: f32) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_cloud_mode(mode);
    let mut camera = Camera::new(Vec3::new(x, 100.0, 0.0), 0.0, 1.2);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("read pixels")
}

/// Pixels that differ from the cloudless frame, summed over a few places
/// (the generated mask is patchy, one spot may be clear sky).
fn cloud_pixels(mode: CloudMode) -> usize {
    [0.0, 400.0, 800.0, 1200.0, 1600.0]
        .into_iter()
        .map(|x| {
            let off = cloud_frame(CloudMode::Off, x);
            let on = cloud_frame(mode, x);
            on.rgba
                .chunks_exact(4)
                .zip(off.rgba.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count()
        })
        .sum()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn clouds_paint_the_sky_in_both_modes() {
    for mode in [CloudMode::Fast, CloudMode::Fancy] {
        let changed = cloud_pixels(mode);
        assert!(changed > 100, "{mode:?} changed {changed} pixels");
    }
}

/// The dome's fog at a pixel follows the true distance to the eye. The bug:
/// the distance was computed at the corners of the dome's 64-block squares
/// and interpolated, so with Fancy fog the triangle edges showed as straight
/// pale wedges that moved with the view.
#[test]
#[ignore = "requires a GPU adapter"]
fn the_dome_fog_follows_the_true_distance_between_its_corners() {
    use umicraft_render::SkyDetails;

    let environment = RenderEnvironment {
        // Only the dome: no sun, moon or stars over the measured pixels.
        sky_details: SkyDetails {
            sky: true,
            stars: false,
            sun_moon: false,
        },
        ..RenderEnvironment::new(0, 6_000, 0.0, 2)
    };
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(environment);
    let mut camera = Camera::new(Vec3::new(0.0, 100.0, 0.0), 0.0, 1.5);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    let image = renderer.read_pixels().expect("read pixels");

    let state = SkyState::at(environment);
    let inverse = camera.view_projection_around_eye().inverse();
    // Two chunks: the sky fog runs linearly from 0 to 32 blocks.
    let fog_end = 32.0;
    for x in [WIDTH / 2, WIDTH / 2 + 20, WIDTH / 2 + 36, WIDTH - 12] {
        let y = HEIGHT / 2;
        // The dome point under the pixel centre, 16 blocks above the eye.
        let ndc_x = (x as f32 + 0.5) / WIDTH as f32 * 2.0 - 1.0;
        let ndc_y = 1.0 - (y as f32 + 0.5) / HEIGHT as f32 * 2.0;
        let near = inverse.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let far = inverse.project_point3(Vec3::new(ndc_x, ndc_y, 0.5));
        let direction = (far - near).normalize();
        let point = direction * (16.0 / direction.y);
        let amount = (point.length() / fog_end).clamp(0.0, 1.0);
        let expected: [i32; 3] = std::array::from_fn(|c| {
            let mixed = state.sky_color[c] + (state.fog.color[c] - state.sky_color[c]) * amount;
            (mixed * 255.0).round() as i32
        });
        let got = pixel(&image, x, y).map(i32::from);
        for channel in 0..3 {
            assert!(
                (got[channel] - expected[channel]).abs() <= 4,
                "pixel {x}: {got:?} vs {expected:?} (fog {amount})"
            );
        }
    }
}
