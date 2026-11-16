// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Weather pass (P7f): rain and snow quads, lit like the terrain and fogged.
// The shader is original; rain.png and snow.png come from the user's
// installation at runtime (spec section 3). The camera block is the one of
// terrain.wgsl (same 112-byte layout), so the renderer shares its uniform.

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
    // Integer world offset of the render origin: positions are sent to the
    // GPU relative to it (`view_proj` is built for that origin), so f32
    // keeps its precision far from the world origin.
    origin: vec3<f32>,
    // The vanilla lightmap, `[sky * 16 + block]` (`lightmap.rs`).
    lightmap: array<vec4<f32>, 256>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var weather_texture: texture_2d<f32>;
@group(1) @binding(1) var weather_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) alpha: f32,
    // Sky and block light, each 0..=15.
    @location(3) light: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) light: vec3<f32>,
    @location(3) fog_target: vec3<f32>,
    @location(4) fog_amount: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let rel_pos = in.position - camera.origin;
    out.clip_position = camera.view_proj * vec4<f32>(rel_pos, 1.0);
    out.uv = in.uv;
    out.alpha = in.alpha;
    let light = lightmap_sample(in.light.y, in.light.x);
    out.light = light;
    let fog_target = camera.fog_color * (0.15 + 0.85 * light);
    out.fog_target = fog_target;
    let span = max(camera.fog_end - camera.fog_start, 0.001);
    let dist = distance(rel_pos, camera.camera_pos);
    let factor = clamp((dist - camera.fog_start) / span, 0.0, 1.0);
    out.fog_amount = factor * factor * (3.0 - 2.0 * factor);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(weather_texture, weather_sampler, in.uv);
    let alpha = texel.a * in.alpha;
    // The reference's alpha test (`alphaFunc(GL_GREATER, 0.1)`).
    if (alpha <= 0.1) {
        discard;
    }
    let lit = texel.rgb * in.light;
    let color = mix(lit, in.fog_target, in.fog_amount);
    return vec4<f32>(color, alpha);
}
