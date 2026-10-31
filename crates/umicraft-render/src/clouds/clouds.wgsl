// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Cloud pass (P7e): the fast and the fancy layer share one vertex format.
// The mesh is static; the frame only shifts it and scrolls the texture, both
// through this uniform. The shader is original; clouds.png comes from the
// user's installation at runtime (spec section 3).

struct Clouds {
    // Camera projection and rotation, without the eye translation.
    view_proj: mat4x4<f32>,
    // xz: added to the mesh position before the scale; y: height of the
    // cloud base above the eye.
    shift: vec4<f32>,
    // xz: scale of the shifted position (1 fast, 12 fancy).
    scale: vec4<f32>,
    // xy: added to the texture coordinate.
    uv_offset: vec4<f32>,
    // rgb: cloud tint; a: cloud opacity (0.8).
    tint: vec4<f32>,
    // rgb fog colour.
    fog_color: vec4<f32>,
    // x fog start, y fog end, in blocks.
    fog: vec4<f32>,
};

@group(0) @binding(0) var<uniform> clouds: Clouds;
@group(0) @binding(1) var cloud_texture: texture_2d<f32>;
@group(0) @binding(2) var cloud_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shade: vec4<f32>,
};

struct VertexOut {
    // Invariant: the depth pre-pass and the colour pass must produce the same
    // depth, or the colour pass would reject its own fragments.
    @builtin(position) @invariant position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) shade: vec3<f32>,
    @location(2) eye_distance: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let relative = vec3<f32>(
        (in.position.x + clouds.shift.x) * clouds.scale.x,
        in.position.y + clouds.shift.y,
        (in.position.z + clouds.shift.z) * clouds.scale.z,
    );
    var out: VertexOut;
    out.position = clouds.view_proj * vec4<f32>(relative, 1.0);
    out.uv = in.uv + clouds.uv_offset.xy;
    out.shade = in.shade.rgb;
    out.eye_distance = length(relative);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(cloud_texture, cloud_sampler, in.uv);
    // The reference's alpha test: a texel without cloud leaves no pixel, in
    // the depth pre-pass too.
    if (texel.a < 0.5) {
        discard;
    }
    let lit = texel.rgb * clouds.tint.rgb * in.shade;
    // The same smoothstep fog as the terrain.
    let span = max(clouds.fog.y - clouds.fog.x, 0.001);
    let amount = smoothstep(clouds.fog.x, clouds.fog.x + span, in.eye_distance);
    return vec4<f32>(mix(lit, clouds.fog_color.rgb, amount), texel.a * clouds.tint.a);
}
