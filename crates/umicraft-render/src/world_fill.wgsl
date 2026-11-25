// SPDX-License-Identifier: MIT OR Apache-2.0

// The filled shapes of mods (API 1.4): translucent triangles in world space,
// positions relative to the render origin, one colour per vertex. The pipeline
// blends them and never writes depth; whether the world hides them is the
// pipeline's depth test.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_brightness: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    gamma: f32,
    fog_density: f32,
    _pad1: f32,
    origin: vec3<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

// Pulls the faces towards the camera so a face lying on a block wins against it.
const DEPTH_BIAS: f32 = 0.00005;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> VertexOut {
    var clip = camera.view_proj * vec4<f32>(position - camera.origin, 1.0);
    clip.z = max(clip.z - DEPTH_BIAS * clip.w, 0.0);
    return VertexOut(clip, color);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}
