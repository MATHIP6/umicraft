// SPDX-License-Identifier: MIT OR Apache-2.0

// Blur of the rendered game behind a menu: a separable nine-tap Gaussian along
// one axis per pass, then a plain copy of the result onto the screen.

struct Params {
    // xy: one texel step along the blurred axis, in texture coordinates.
    values: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle that covers the whole target.
@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> VertexOut {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

fn tap(uv: vec2<f32>, offset: vec2<f32>, weight: f32) -> vec3<f32> {
    return (textureSample(src, samp, uv + offset).rgb
        + textureSample(src, samp, uv - offset).rgb) * weight;
}

@fragment
fn fs_blur(in: VertexOut) -> @location(0) vec4<f32> {
    let step = params.values.xy;
    var color = textureSample(src, samp, in.uv).rgb * 0.2270270270;
    color += tap(in.uv, step * 1.0, 0.1945945946);
    color += tap(in.uv, step * 2.0, 0.1216216216);
    color += tap(in.uv, step * 3.0, 0.0540540541);
    color += tap(in.uv, step * 4.0, 0.0162162162);
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_blit(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(src, samp, in.uv).rgb, 1.0);
}

// The blurred picture over the sharp one, as opaque as `values.z`: the blur
// fades in and out with it.
@fragment
fn fs_blit_mix(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(src, samp, in.uv).rgb, params.values.z);
}
