// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Title-screen panorama (P10-D2): six textured quads of a cube seen from
// inside, after GuiMainMenu.renderSkybox. The shader is original; the six
// pictures come from the user's installation at runtime (spec section 3).

struct Panorama {
    // Projection times the rotation of the current frame.
    clip_from_cube: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> panorama: Panorama;
@group(0) @binding(1) var face: texture_2d<f32>;
@group(0) @binding(2) var face_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let clip = panorama.clip_from_cube * vec4<f32>(in.position, 1.0);
    // Depth 0 everywhere: the world passes drawn after the panorama (clouds,
    // particles, ...) fail their depth test and cannot show through it.
    out.position = vec4<f32>(clip.xy, 0.0, clip.w);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(face, face_sampler, in.uv).rgb, 1.0);
}
