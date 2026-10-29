// SPDX-License-Identifier: MIT OR Apache-2.0
//
// GUI overlay shader: NDC quads sampling an RGBA GUI texture atlas. The vertex
// colour tints the texel; alpha blending is handled by the pipeline.

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

@group(0) @binding(0) var gui_atlas: texture_2d<f32>;
@group(0) @binding(1) var gui_sampler: sampler;

// Vertex colours are authored as sRGB values (vanilla `0xFF5555` and the like)
// but the target is an sRGB surface that encodes the linear output, so they are
// decoded here; otherwise every tint comes out washed out.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = vec4<f32>(srgb_to_linear(in.color.rgb), in.color.a);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(gui_atlas, gui_sampler, in.uv);
    return texel * in.color;
}
