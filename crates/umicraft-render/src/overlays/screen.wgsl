// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Screen overlays (P8-f): quads already in clip space that sample either a
// block atlas layer (group 0, the terrain's) or one of the three pictures of
// group 1. Original shader.

@group(0) @binding(1) var block_textures: texture_2d_array<f32>;
@group(0) @binding(2) var block_sampler: sampler;
@group(1) @binding(0) var underwater_texture: texture_2d<f32>;
@group(1) @binding(1) var pumpkin_texture: texture_2d<f32>;
@group(1) @binding(2) var vignette_texture: texture_2d<f32>;
@group(1) @binding(3) var picture_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    // Atlas layer when not negative, else -1 water, -2 pumpkin, -3 vignette.
    // The second value is 1.0 when the texture alpha is ignored.
    @location(2) source: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) source: vec2<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.source = in.source;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Every picture is sampled outside any branch: derivatives need uniform
    // control flow.
    let layer = u32(max(in.source.x, 0.0));
    let atlas = textureSample(block_textures, block_sampler, in.uv, layer);
    let water = textureSample(underwater_texture, picture_sampler, in.uv);
    let pumpkin = textureSample(pumpkin_texture, picture_sampler, in.uv);
    let vignette = textureSample(vignette_texture, picture_sampler, in.uv);
    var texel = atlas;
    if (in.source.x < -2.5) {
        texel = vignette;
    } else if (in.source.x < -1.5) {
        texel = pumpkin;
    } else if (in.source.x < -0.5) {
        texel = water;
    }
    let alpha = select(texel.a, 1.0, in.source.y > 0.5);
    return vec4<f32>(texel.rgb * in.color.rgb, alpha * in.color.a);
}
