// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the rain and snow pass. The generated fallback
//! pictures are used, so no official assets are required.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_weather -- --ignored`
//! on a machine with a wgpu adapter.

use std::sync::Arc;

use glam::Vec3;
use umicraft_render::{
    Camera, Image, Overlay, PrecipitationCell, PrecipitationField, RenderEnvironment, Renderer,
};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;

/// A field of columns around the origin with the given base temperature.
fn field(temperature: f32) -> Arc<PrecipitationField> {
    let mut field = PrecipitationField::new(0, 0);
    for z in -12..=12 {
        for x in -12..=12 {
            field.set(
                x,
                z,
                PrecipitationCell {
                    height: 64,
                    temperature,
                    sky_light: 15,
                    block_light: 0,
                },
            );
        }
    }
    Arc::new(field)
}

/// Renders one frame from the origin at eye height 66, looking along +z.
fn frame(rain_strength: f32, field: Arc<PrecipitationField>) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(RenderEnvironment {
        rain_strength,
        ..RenderEnvironment::default()
    });
    renderer.set_precipitation(field, true);
    renderer.set_cloud_ticks(40.25);
    let mut camera = Camera::new(Vec3::new(0.5, 66.0, 0.5), 0.0, -0.2);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("read pixels")
}

/// How many pixels differ between two frames.
fn changed_pixels(a: &Image, b: &Image) -> usize {
    a.rgba
        .chunks_exact(4)
        .zip(b.rgba.chunks_exact(4))
        .filter(|(x, y)| x[..3] != y[..3])
        .count()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn rain_and_snow_change_the_picture_and_dry_weather_does_not() {
    let dry = frame(0.0, field(0.8));
    assert_eq!(changed_pixels(&dry, &frame(0.0, Arc::default())), 0);
    let rain = frame(1.0, field(0.8));
    let snow = frame(1.0, field(0.0));
    assert!(changed_pixels(&dry, &rain) > 50, "rain draws streaks");
    assert!(changed_pixels(&dry, &snow) > 20, "snow draws flakes");
    assert!(changed_pixels(&rain, &snow) > 20, "they differ");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_drizzle_is_fainter_than_a_downpour() {
    let dry = frame(0.0, field(0.8));
    let downpour = changed_pixels(&dry, &frame(1.0, field(0.8)));
    let drizzle = changed_pixels(&dry, &frame(0.3, field(0.8)));
    assert!(drizzle <= downpour, "{drizzle} {downpour}");
}
