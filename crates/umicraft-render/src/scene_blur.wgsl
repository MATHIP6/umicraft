// SPDX-License-Identifier: MIT OR Apache-2.0

// Blur of the rendered game behind a menu: a dual filter (Kawase). Each
// downsample takes five taps of the larger picture, each upsample eight taps of
// the smaller one; the blurred picture is then graded (darker, a little less
// saturated, with a vignette on the overlay) over the sharp one.

struct Params {
    // xy: half a texel of the source, in texture coordinates; z: strength of
    // the blurred picture over the sharp one; w: tap offset of the filter.
    values: vec4<f32>,
    // x: brightness; y: saturation; z: vignette (0 or 1).
    grade: vec4<f32>,
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

fn at(uv: vec2<f32>, offset: vec2<f32>) -> vec3<f32> {
    return textureSample(src, samp, uv + offset).rgb;
}

@fragment
fn fs_down(in: VertexOut) -> @location(0) vec4<f32> {
    let h = params.values.xy * params.values.w;
    var sum = at(in.uv, vec2<f32>(0.0)) * 4.0;
    sum += at(in.uv, -h);
    sum += at(in.uv, h);
    sum += at(in.uv, vec2<f32>(h.x, -h.y));
    sum += at(in.uv, vec2<f32>(-h.x, h.y));
    return vec4<f32>(sum / 8.0, 1.0);
}

@fragment
fn fs_up(in: VertexOut) -> @location(0) vec4<f32> {
    let h = params.values.xy * params.values.w;
    var sum = at(in.uv, vec2<f32>(-h.x * 2.0, 0.0));
    sum += at(in.uv, vec2<f32>(-h.x, h.y)) * 2.0;
    sum += at(in.uv, vec2<f32>(0.0, h.y * 2.0));
    sum += at(in.uv, vec2<f32>(h.x, h.y)) * 2.0;
    sum += at(in.uv, vec2<f32>(h.x * 2.0, 0.0));
    sum += at(in.uv, vec2<f32>(h.x, -h.y)) * 2.0;
    sum += at(in.uv, vec2<f32>(0.0, -h.y * 2.0));
    sum += at(in.uv, vec2<f32>(-h.x, -h.y)) * 2.0;
    return vec4<f32>(sum / 12.0, 1.0);
}

@fragment
fn fs_blit(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(src, samp, in.uv).rgb, 1.0);
}

// The blurred picture, graded, over the sharp one as opaque as `values.z`: the
// blur fades in and out with it.
@fragment
fn fs_blit_mix(in: VertexOut) -> @location(0) vec4<f32> {
    var color = textureSample(src, samp, in.uv).rgb;
    let grey = vec3<f32>(dot(color, vec3<f32>(0.2126, 0.7152, 0.0722)));
    color = mix(grey, color, params.grade.y) * params.grade.x;
    // The vignette of the overlay: from 10 % of a dark tint at the centre
    // (50 %, 45 %) to 50 % at three quarters of the way to the edge.
    let reach = clamp(length(in.uv - vec2<f32>(0.5, 0.45)) / 0.75, 0.0, 1.0);
    let scrim = mix(0.10, 0.50, reach) * params.grade.z;
    color = mix(color, vec3<f32>(0.039, 0.047, 0.055), scrim);
    return vec4<f32>(color, params.values.z);
}
