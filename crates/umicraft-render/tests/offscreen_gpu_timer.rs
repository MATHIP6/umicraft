// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the GPU timestamps (F2-0b).
//!
//! Run with `cargo test -p umicraft-render --test offscreen_gpu_timer -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_render::{Camera, Overlay, Renderer};

#[test]
#[ignore = "needs a wgpu adapter"]
fn timings_arrive_after_a_few_frames_or_the_adapter_reports_unsupported() {
    let mut renderer = Renderer::new_offscreen(128, 96).expect("offscreen renderer");
    let mut camera = Camera::new(Vec3::new(0.0, 100.0, 0.0), 0.0, 0.0);
    camera.set_aspect(128, 96);
    for _ in 0..8 {
        renderer
            .render(&camera, &Overlay::new(128, 96))
            .expect("render");
        renderer.read_pixels().expect("read pixels");
    }
    if !renderer.gpu_timing_supported() {
        println!("timestamps unsupported");
        assert!(renderer.gpu_timings().is_none());
        return;
    }
    let timings = renderer.gpu_timings().expect("a frame came back");
    println!("{timings:?}");
    assert!(timings.total_ms() > 0.0, "{timings:?}");
    assert!(timings.total_ms() < 1000.0, "{timings:?}");
}
