// SPDX-License-Identifier: MIT OR Apache-2.0

// Block selection outline: flat-coloured world-space lines. The colour is
// vanilla's (0, 0, 0, 0.4), alpha-blended by the pipeline.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_brightness: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    gamma: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return camera.view_proj * vec4<f32>(position, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.4);
}
