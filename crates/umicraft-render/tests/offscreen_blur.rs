// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the background blur (the mods menu).
//!
//! The GPU validates the blur passes, their bind groups and their formats only
//! when they run, so this runs them: the frame must come out whole (not black,
//! not an error), the blur must keep the colours of the picture (its taps sum
//! to one) and turning it off must bring the plain frame back.
//!
//! Run with
//! `cargo test -p umicraft-render --test offscreen_blur -- --ignored`
//! on a machine with a wgpu adapter.

use umicraft_render::{Camera, Image, Overlay, RenderEnvironment, Renderer};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;

fn frame(renderer: &mut Renderer, camera: &Camera) -> Image {
    let overlay = Overlay::new(WIDTH, HEIGHT);
    renderer.render(camera, &overlay).expect("render the frame");
    renderer.read_pixels().expect("read back the frame")
}

/// The average of one colour `channel` (0 red, 1 green, 2 blue) over a frame.
fn mean(image: &Image, channel: usize) -> f64 {
    let values: Vec<f64> = image
        .rgba
        .chunks_exact(4)
        .map(|pixel| f64::from(pixel[channel]))
        .collect();
    values.iter().sum::<f64>() / values.len() as f64
}

/// The largest difference between two pixels of two frames, over every channel.
fn largest_difference(a: &Image, b: &Image) -> u8 {
    a.rgba
        .iter()
        .zip(&b.rgba)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_background_blur_runs_keeps_the_colours_and_turns_off() {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(RenderEnvironment::default());
    let mut camera = Camera::new(glam::Vec3::new(0.0, 80.0, 0.0), 0.0, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);

    let plain = frame(&mut renderer, &camera);
    renderer.set_background_blur(1.0);
    let blurred = frame(&mut renderer, &camera);
    renderer.set_background_blur(0.0);
    let again = frame(&mut renderer, &camera);

    // A picture that is not black went through the four passes and the copy.
    assert!(plain.rgba.iter().any(|&byte| byte > 40), "the scene is lit");
    // The blur adds or loses no light: every channel keeps its average. (A
    // pixel can move more, where the picture had a gradient.)
    for channel in 0..3 {
        let (before, after) = (mean(&plain, channel), mean(&blurred, channel));
        assert!(
            (before - after).abs() < 3.0,
            "channel {channel} went from {before:.1} to {after:.1}"
        );
    }
    assert_eq!(
        largest_difference(&plain, &again),
        0,
        "the blur is off again"
    );
}
