// SPDX-License-Identifier: MIT OR Apache-2.0
//
// World border pass (P7g), mirroring RenderGlobal.renderWorldBorder: the
// force-field texture modulated by the status colour, drawn additively
// (blend SRC_ALPHA, ONE) without depth writes. The shader is original;
// forcefield.png comes from the user's installation at runtime (spec section 3).

struct Border {
    // Camera projection and rotation, without the eye translation.
    view_proj: mat4x4<f32>,
    // rgb: status colour; a: fade (1 - closeness)^4.
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> border: Border;
@group(0) @binding(1) var forcefield: texture_2d<f32>;
@group(0) @binding(2) var forcefield_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shade: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = border.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(forcefield, forcefield_sampler, in.uv) * border.tint;
    // The reference's alpha test (alphaFunc(GL_GREATER, 0.1)).
    if (texel.a <= 0.1) {
        discard;
    }
    return texel;
}
