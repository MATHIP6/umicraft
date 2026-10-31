// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the world border pass, with the generated
//! fallback pattern (no official assets needed).
//!
//! Run with `cargo test -p umicraft-render --test offscreen_border -- --ignored`
//! on a machine with a wgpu adapter. Set `UMICRAFT_TEST_OUT` to a directory to
//! also get the frames as PPM files.

use glam::Vec3;
use umicraft_render::{
    BorderStatus, BorderView, Camera, Image, Overlay, RenderEnvironment, Renderer,
};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 100;

fn border(status: BorderStatus) -> BorderView {
    BorderView {
        center: (0.0, 0.0),
        diameter: 1000.0,
        portal_boundary: 29_999_984,
        status,
    }
}

/// Renders from x = 480 (20 blocks inside the east wall) looking along `yaw`.
fn frame(view: Option<BorderView>, yaw: f32, name: &str) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(RenderEnvironment::default());
    renderer.set_world_border(view, 0.75);
    let mut camera = Camera::new(Vec3::new(480.0, 64.0, 0.0), yaw, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    let image = renderer.read_pixels().expect("read pixels");
    if let Ok(dir) = std::env::var("UMICRAFT_TEST_OUT") {
        let mut ppm = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
        ppm.extend(image.rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
        std::fs::write(format!("{dir}/{name}.ppm"), ppm).expect("write the frame");
    }
    image
}

fn differing(a: &Image, b: &Image) -> usize {
    a.rgba
        .chunks_exact(4)
        .zip(b.rgba.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}

const EAST: f32 = std::f32::consts::FRAC_PI_2;

#[test]
#[ignore = "requires a GPU adapter"]
fn the_wall_shows_when_facing_it_and_not_when_looking_away() {
    let bare = frame(None, EAST, "border_none");
    let facing = frame(Some(border(BorderStatus::Stationary)), EAST, "border_blue");
    assert!(
        differing(&bare, &facing) > 1000,
        "the wall covers much of the view"
    );
    let away_bare = frame(None, -EAST, "away_none");
    let away = frame(Some(border(BorderStatus::Stationary)), -EAST, "away_blue");
    assert_eq!(differing(&away_bare, &away), 0);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_status_picks_the_tint() {
    let mean = |image: &Image| -> [u64; 3] {
        let mut sum = [0; 3];
        for p in image.rgba.chunks_exact(4) {
            for c in 0..3 {
                sum[c] += u64::from(p[c]);
            }
        }
        sum
    };
    let bare = mean(&frame(None, EAST, "t_none"));
    let gain = |status, name| {
        let m = mean(&frame(Some(border(status)), EAST, name));
        [0, 1, 2].map(|c| m[c].saturating_sub(bare[c]))
    };
    let red = gain(BorderStatus::Shrinking, "border_red");
    let green = gain(BorderStatus::Growing, "border_green");
    assert!(red[0] > red[2] * 2, "shrinking adds red: {red:?}");
    assert!(green[1] > green[0] * 2, "growing adds green: {green:?}");
}
