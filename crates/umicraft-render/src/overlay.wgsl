// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Overlay shader: NDC quads sampling an R8 glyph atlas. The vertex colour is
// straight-alpha RGBA, blended by the pipeline.

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(font_atlas, font_sampler, in.uv).r;
    if (coverage < 0.5) {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
