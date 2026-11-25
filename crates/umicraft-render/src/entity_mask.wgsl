// SPDX-License-Identifier: MIT OR Apache-2.0

// The entity mask of the post passes (API 1.4): the models of the entities a mod
// marked, in the flat colour it gave them. The vertices are entity vertices whose
// `normal` holds the colour and whose light word holds the opacity in bits 0..=7;
// bits 27..=31 keep the alpha cutoff of the layer, and transparent texels of the
// skin are dropped as the entity pass drops them, so the shape is the drawn one.

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
@group(1) @binding(0) var entity_texture: texture_2d<f32>;
@group(1) @binding(1) var entity_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) light: u32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) cutoff: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position - camera.origin, 1.0);
    out.uv = in.uv;
    out.color = vec4<f32>(in.normal, f32(in.light & 0xFFu) / 255.0);
    out.cutoff = f32(in.light >> 27u) / 31.0;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(entity_texture, entity_sampler, in.uv);
    if (tex.a < max(0.1, in.cutoff)) {
        discard;
    }
    return in.color;
}
