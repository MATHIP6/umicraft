// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Terrain shader: textured cube faces from a 2D texture array, simple
// axis-based directional shading, and distance fog toward the sky colour.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var block_textures: texture_2d_array<f32>;
@group(0) @binding(2) var block_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: u32,
    // Packed VertexLight: block/sky/AO nibbles and the shade bit. The M6
    // terrain workstream consumes it; the attribute is declared now so the
    // vertex layout is frozen before the parallel split.
    @location(4) light: u32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) layer: u32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.normal = in.normal;
    out.uv = in.uv;
    out.layer = in.layer;
    return out;
}

const SKY: vec3<f32> = vec3<f32>(0.55, 0.72, 0.95);

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(block_textures, block_sampler, in.uv, i32(in.layer));
    if (texel.a < 0.5) {
        discard;
    }
    // Axis shading keeps faces legible without a light engine (M2 scope).
    var shade = 0.8;
    if (in.normal.y > 0.5) {
        shade = 1.0;
    } else if (in.normal.y < -0.5) {
        shade = 0.45;
    } else if (abs(in.normal.x) > 0.5) {
        shade = 0.65;
    }
    let fog = smoothstep(96.0, 220.0, distance(in.world_position, camera.camera_pos));
    return vec4<f32>(mix(texel.rgb * shade, SKY, fog), 1.0);
}
