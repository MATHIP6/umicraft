// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the particle pass. The generated fallback sheet is
//! used, so no official assets are required.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_particles -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_render::{Camera, Image, Overlay, ParticleSprite, Renderer};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;

/// A white sprite two blocks ahead of the camera.
fn sprite(color: [f32; 4]) -> ParticleSprite {
    ParticleSprite {
        center: Vec3::new(0.5, 66.0, 2.5),
        half_size: 0.5,
        uv_min: [0.0, 0.0],
        uv_max: [1.0 / 16.0, 1.0 / 16.0],
        atlas_layer: None,
        color,
        light: [15.0, 15.0],
        flat: false,
    }
}

/// Renders one frame looking along +z (yaw 0 faces -z in this camera, so it
/// turns by pi).
fn frame(sprites: &[ParticleSprite]) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    let mut camera = Camera::new(Vec3::new(0.5, 66.0, 0.5), std::f32::consts::PI, 0.0);
    // Sprite centres are relative to the render origin of the camera.
    let origin = camera.render_origin().as_vec3();
    let relative: Vec<ParticleSprite> = sprites
        .iter()
        .map(|sprite| ParticleSprite {
            center: sprite.center - origin,
            ..*sprite
        })
        .collect();
    renderer.set_particles(&relative);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("read pixels")
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
fn a_particle_changes_the_picture_and_none_does_not() {
    let empty = frame(&[]);
    assert_eq!(changed_pixels(&empty, &frame(&[])), 0);
    assert!(changed_pixels(&empty, &frame(&[sprite([1.0; 4])])) > 20);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_fully_transparent_particle_draws_nothing() {
    let empty = frame(&[]);
    assert_eq!(
        changed_pixels(&empty, &frame(&[sprite([1.0, 1.0, 1.0, 0.0])])),
        0
    );
}
