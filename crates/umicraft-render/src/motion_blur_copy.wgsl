// SPDX-License-Identifier: MIT OR Apache-2.0

// Puts the blurred world on the screen, inside the pass that then draws the hand
// and the HUD sharp over it. A separate shader and layout from `motion_blur.wgsl`:
// that pass keeps the depth buffer as its attachment, so it cannot also read it.

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

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

@fragment
fn fs_copy(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(picture, samp, in.uv, 0.0).rgb, 1.0);
}
