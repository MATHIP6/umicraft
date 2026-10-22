// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Terrain shader: textured cube faces from a 2D texture array, smooth
// block/sky lighting with baked ambient occlusion, 1.8 axis face shading and
// distance fog toward the sky/fog colour.
//
// The light model is the M6 contract in
// `docs/decisions/0008-m6-lighting-sky-transparency.md` §3, sourced from the
// 1.8 brightness table (`WorldProvider.generateLightBrightnessTable`) and the
// lightmap combination in `EntityRenderer.updateLightmap`:
//   b(level) = (level/15) / (3 * (1 - level/15) + 1)
//   sky      = b(sky) * sun_brightness
//   block    = b(block)
//   light    = min(1, sky + block)
//   ao       = 0.2 + 0.8 * (ao_nibble / 15)
// The five `VertexLight` fields are packed in the fifth vertex attribute:
// bits 0..=3 block, 4..=7 sky, 8..=11 AO, bit 12 shade.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    // Sun-light factor in 0..=1 from `SkyState`, so full sky light reads dark
    // at night and in the Nether/End.
    sun_brightness: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    // Explicit tail padding keeps the Rust `CameraUniform` and this struct at
    // the same 112-byte, 16-byte-aligned layout.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var block_textures: texture_2d_array<f32>;
@group(0) @binding(2) var block_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: u32,
    // Packed VertexLight: block/sky/AO nibbles and the shade bit.
    @location(4) light: u32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) layer: u32,
    // Interpolated smooth lighting, AO and face shade. Interpolating the
    // baked per-corner values reproduces 1.8 smooth lighting across the quad.
    @location(3) light: f32,
    @location(4) ao: f32,
    @location(5) shade: f32,
};

// 1.8 light-brightness curve. The input is a normalized nibble in 0..=1.
fn light_brightness(level: f32) -> f32 {
    return level / (3.0 * (1.0 - level) + 1.0);
}

// 1.8 face shading: top is full bright, bottom darkest, the two horizontal
// axes in between. Used only when the mesher sets the vertex `shade` bit;
// crosses, plants and torches keep their model brightness.
fn face_shade(normal: vec3<f32>) -> f32 {
    if (normal.y > 0.5) {
        return 1.0;
    }
    if (normal.y < -0.5) {
        return 0.45;
    }
    if (abs(normal.x) > 0.5) {
        return 0.65;
    }
    return 0.8;
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.uv = in.uv;
    out.layer = in.layer;

    let block = light_brightness(f32(in.light & 0xFu) / 15.0);
    let sky = light_brightness(f32((in.light >> 4u) & 0xFu) / 15.0) * camera.sun_brightness;
    out.light = min(1.0, sky + block);
    out.ao = 0.2 + 0.8 * (f32((in.light >> 8u) & 0xFu) / 15.0);

    let shaded = (in.light & (1u << 12u)) != 0u;
    if (shaded) {
        out.shade = face_shade(in.normal);
    } else {
        out.shade = 1.0;
    }
    return out;
}

// Applies the sampled texture light/AO/shade and the distance fog. The fog
// target is darkened by the fragment light so a cave does not show a bright
// sky-coloured haze (`ADR-0008` §3).
fn shade_fragment(in: VertexOut, texel: vec4<f32>) -> vec3<f32> {
    let lit = texel.rgb * in.light * in.ao * in.shade;
    let fog_target = camera.fog_color * (0.15 + 0.85 * in.light);
    let fog_span = max(camera.fog_end - camera.fog_start, 0.001);
    let distance = distance(in.world_position, camera.camera_pos);
    let fog = smoothstep(camera.fog_start, camera.fog_start + fog_span, distance);
    return mix(lit, fog_target, fog);
}

// Opaque pass: alpha cutout, depth write on. Returns opaque alpha so the
// REPLACE blend state stays well-defined.
@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(block_textures, block_sampler, in.uv, i32(in.layer));
    if (texel.a < 0.5) {
        discard;
    }
    return vec4<f32>(shade_fragment(in, texel), 1.0);
}

// Transparent pass: water/glass/leaves keep their real texture alpha and are
// blended with SrcAlpha/OneMinusSrcAlpha. No cutout: water uses partial alpha.
@fragment
fn fs_transparent(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(block_textures, block_sampler, in.uv, i32(in.layer));
    return vec4<f32>(shade_fragment(in, texel), texel.a);
}
