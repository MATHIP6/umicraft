// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The blur of GuiMainMenu.rotateAndBlurSkybox on the 256 x 256 capture: a
// three-tap box filter along one axis, repeated seven times alternately
// along x and y, then a stretched, quarter-turned blit to the screen.

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
// blur: xy = one texel along the blurred axis. blit: xy = the magnification
// (kx, ky) of the capture on screen.
@group(0) @binding(2) var<uniform> params: vec4<f32>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// One triangle covering the whole target, at depth 0.
@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> VertexOut {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    let ndc = corner * 2.0 - 1.0;
    var out: VertexOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

fn capture_uv(ndc: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

@fragment
fn fs_blur(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = capture_uv(in.ndc);
    let a = textureSample(image, image_sampler, uv - params.xy).rgb;
    let b = textureSample(image, image_sampler, uv).rgb;
    let c = textureSample(image, image_sampler, uv + params.xy).rgb;
    return vec4<f32>((a + b + c) / 3.0, 1.0);
}

// Screen x is capture -y and screen y is capture x, scaled by the crop
// (the same mapping as the matrix `fit` of the unblurred pass).
@fragment
fn fs_blit(in: VertexOut) -> @location(0) vec4<f32> {
    let capture = vec2<f32>(in.ndc.y / params.y, -in.ndc.x / params.x);
    return vec4<f32>(textureSample(image, image_sampler, capture_uv(capture)).rgb, 1.0);
}
