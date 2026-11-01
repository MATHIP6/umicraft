// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Particle pass (P8-a): billboards lit and fogged like the terrain. The
// shader is original. Group 0 is the terrain's (camera, block atlas array,
// sampler); group 1 holds the `particles.png` sheet. The fog falloff comes
// from fog.wgsl, concatenated in front.

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
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var block_textures: texture_2d_array<f32>;
@group(0) @binding(2) var block_sampler: sampler;
@group(1) @binding(0) var sheet_texture: texture_2d<f32>;
@group(1) @binding(1) var sheet_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    // Block atlas layer, or a negative value for the sheet.
    @location(2) layer: f32,
    @location(3) color: vec4<f32>,
    // Block and sky light, each 0..=15.
    @location(4) light: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) layer: f32,
    @location(3) color: vec4<f32>,
    @location(4) light: f32,
};

fn light_brightness(level: f32) -> f32 {
    return level / (3.0 * (1.0 - level) + 1.0);
}

fn gamma_correct(value: f32) -> f32 {
    let inv = 1.0 - value;
    let corrected = 1.0 - inv * inv * inv * inv;
    return value * (1.0 - camera.gamma) + corrected * camera.gamma;
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.uv = in.uv;
    out.layer = in.layer;
    out.color = in.color;
    let block = light_brightness(in.light.x / 15.0);
    let sky = light_brightness(in.light.y / 15.0) * camera.sun_brightness;
    out.light = gamma_correct(min(1.0, sky + block));
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Both samples are taken outside any branch: the derivatives need
    // uniform control flow.
    let sheet = textureSample(sheet_texture, sheet_sampler, in.uv);
    let layer = u32(max(in.layer, 0.0));
    let atlas = textureSample(block_textures, block_sampler, in.uv, layer);
    var texel = sheet;
    if (in.layer >= 0.0) {
        texel = atlas;
    }
    let alpha = texel.a * in.color.a;
    // The reference's alpha test (`alphaFunc(GL_GREATER, 1/255)`).
    if (alpha <= 0.003921569) {
        discard;
    }
    let lit = texel.rgb * in.color.rgb * in.light;
    let fog_target = camera.fog_color * (0.15 + 0.85 * in.light);
    let fog = fog_amount(
        distance(in.world_position, camera.camera_pos),
        camera.fog_start,
        camera.fog_end,
        camera.fog_density,
    );
    return vec4<f32>(mix(lit, fog_target, fog), alpha);
}
