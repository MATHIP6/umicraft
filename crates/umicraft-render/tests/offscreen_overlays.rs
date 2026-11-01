// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the screen overlays. The generated fallback
//! pictures and atlas are used, so no official assets are required.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_overlays -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_render::{Camera, Image, Overlay, Portal, Renderer, ScreenOverlays, Underwater};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;

/// Renders one frame with `overlays`.
fn frame(overlays: ScreenOverlays) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_screen_overlays(overlays);
    let mut camera = Camera::new(Vec3::new(0.5, 66.0, 0.5), std::f32::consts::PI, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("read pixels")
}

/// The RGB of the pixel at `(x, y)`.
fn pixel(image: &Image, x: u32, y: u32) -> [u8; 3] {
    let at = ((y * WIDTH + x) * 4) as usize;
    [image.rgba[at], image.rgba[at + 1], image.rgba[at + 2]]
}

fn brightness(pixel: [u8; 3]) -> u32 {
    pixel.iter().map(|&c| u32::from(c)).sum()
}

fn changed_pixels(a: &Image, b: &Image) -> usize {
    a.rgba
        .chunks_exact(4)
        .zip(b.rgba.chunks_exact(4))
        .filter(|(x, y)| x[..3] != y[..3])
        .count()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn no_overlay_draws_nothing() {
    let none = frame(ScreenOverlays::default());
    assert_eq!(changed_pixels(&none, &frame(ScreenOverlays::default())), 0);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_vignette_darkens_the_corners_and_spares_the_middle() {
    let none = frame(ScreenOverlays::default());
    let shaded = frame(ScreenOverlays {
        vignette: Some([1.0; 3]),
        ..ScreenOverlays::default()
    });
    assert!(brightness(pixel(&shaded, 1, 1)) < brightness(pixel(&none, 1, 1)));
    assert_eq!(
        pixel(&shaded, WIDTH / 2, HEIGHT / 2),
        pixel(&none, WIDTH / 2, HEIGHT / 2)
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_pumpkin_frames_the_view() {
    let none = frame(ScreenOverlays::default());
    let pumpkin = frame(ScreenOverlays {
        pumpkin: true,
        ..ScreenOverlays::default()
    });
    assert_ne!(pixel(&pumpkin, 1, 1), pixel(&none, 1, 1));
    assert_eq!(
        pixel(&pumpkin, WIDTH / 2, HEIGHT / 2),
        pixel(&none, WIDTH / 2, HEIGHT / 2)
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_water_veil_covers_every_pixel_and_a_dark_wall_blackens_it() {
    let none = frame(ScreenOverlays::default());
    let water = frame(ScreenOverlays {
        underwater: Some(Underwater {
            brightness: 1.0,
            yaw_degrees: 10.0,
            pitch_degrees: 5.0,
        }),
        ..ScreenOverlays::default()
    });
    assert!(changed_pixels(&none, &water) > (WIDTH * HEIGHT) as usize * 9 / 10);
    let wall = frame(ScreenOverlays {
        in_wall: Some(0),
        ..ScreenOverlays::default()
    });
    // Tenth-intensity sprite over the whole screen: the corner is nearly black.
    assert!(brightness(pixel(&wall, 1, 1)) < 3 * 40);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn fire_and_portal_change_the_picture() {
    let none = frame(ScreenOverlays::default());
    let fire = frame(ScreenOverlays {
        fire: Some(0),
        ..ScreenOverlays::default()
    });
    assert!(changed_pixels(&none, &fire) > 50);
    let portal = |alpha| {
        frame(ScreenOverlays {
            portal: Some(Portal { layer: 0, alpha }),
            ..ScreenOverlays::default()
        })
    };
    assert_eq!(changed_pixels(&none, &portal(0.0)), 0);
    assert!(changed_pixels(&none, &portal(0.6)) > (WIDTH * HEIGHT) as usize / 2);
}
