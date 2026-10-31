// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Geometry pass of the sky (P7e): the dome, the sunrise fan, the sun, the
// moon, the stars, the void plane and the End cube, all drawn from the same
// vertex format. One uniform slot per draw (dynamic offset) carries the
// matrices and the per-draw tint, fog and texture switches. The shader is
// original; textures come from the user's installation at runtime (spec §3).

struct Slot {
    // Camera projection and rotation, without the eye translation.
    view_proj: mat4x4<f32>,
    // Placement of the mesh relative to the eye.
    model: mat4x4<f32>,
    // Multiplies the vertex colour.
    tint: vec4<f32>,
    // rgb fog colour.
    fog_color: vec4<f32>,
    // x textured (1/0), y fog on (1/0).
    params: vec4<f32>,
    // x fog start, y fog end in blocks, z exponential density (`fog_amount`).
    fog: vec4<f32>,
};

@group(0) @binding(0) var<uniform> slot: Slot;
@group(0) @binding(1) var sky_texture: texture_2d<f32>;
@group(0) @binding(2) var sky_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Distance from the eye, the input of the linear fog.
    @location(2) eye_distance: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let relative = slot.model * vec4<f32>(in.position, 1.0);
    var out: VertexOut;
    out.position = slot.view_proj * relative;
    out.uv = in.uv;
    out.color = in.color * slot.tint;
    out.eye_distance = length(relative.xyz);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Sampled unconditionally so the derivatives stay in uniform control
    // flow; the switch picks the result.
    let texel = textureSample(sky_texture, sky_sampler, in.uv);
    var color = in.color;
    if (slot.params.x > 0.5) {
        color = color * texel;
    }
    if (slot.params.y > 0.5) {
        // `setupFog(-1)`: the dome's own range, or the medium's exponential fog.
        let amount = fog_amount(in.eye_distance, slot.fog.x, slot.fog.y, slot.fog.z);
        color = vec4<f32>(mix(color.rgb, slot.fog_color.rgb, amount), color.a);
    }
    return color;
}
