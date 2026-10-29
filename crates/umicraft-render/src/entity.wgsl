// SPDX-License-Identifier: MIT OR Apache-2.0

// Entity shader: textured articulated models, face shading, block/sky lighting,
// alpha-tested cutout discard, and distance fog matching SkyState.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_brightness: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    gamma: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var entity_texture: texture_2d<f32>;
@group(1) @binding(1) var entity_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) light: u32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
    @location(3) shade: f32,
    @location(4) @interpolate(flat) hurt: f32,
    @location(5) @interpolate(flat) flash: f32,
    @location(6) @interpolate(flat) dye: vec3<f32>,
};

// Dye colours of the sheep wool and the wolf collar, by metadata value
// (MCP `EntitySheep.DYE_TO_RGB`).
var<private> DYES = array<vec3<f32>, 16>(
    vec3<f32>(1.0, 1.0, 1.0),
    vec3<f32>(0.85, 0.5, 0.2),
    vec3<f32>(0.7, 0.3, 0.85),
    vec3<f32>(0.4, 0.6, 0.85),
    vec3<f32>(0.9, 0.9, 0.2),
    vec3<f32>(0.5, 0.8, 0.1),
    vec3<f32>(0.95, 0.5, 0.65),
    vec3<f32>(0.3, 0.3, 0.3),
    vec3<f32>(0.6, 0.6, 0.6),
    vec3<f32>(0.3, 0.5, 0.6),
    vec3<f32>(0.5, 0.25, 0.7),
    vec3<f32>(0.2, 0.3, 0.7),
    vec3<f32>(0.4, 0.3, 0.2),
    vec3<f32>(0.4, 0.5, 0.2),
    vec3<f32>(0.6, 0.2, 0.2),
    vec3<f32>(0.1, 0.1, 0.1),
);

fn light_brightness(level: f32) -> f32 {
    return level / (3.0 * (1.0 - level) + 1.0);
}

fn gamma_correct(value: f32) -> f32 {
    let inv = 1.0 - value;
    let corrected = 1.0 - inv * inv * inv * inv;
    return value * (1.0 - camera.gamma) + corrected * camera.gamma;
}

fn face_shade(normal: vec3<f32>) -> f32 {
    let ax = abs(normal.x);
    let ay = abs(normal.y);
    let az = abs(normal.z);
    if (ay >= ax && ay >= az) {
        if (normal.y > 0.0) { return 1.0; } else { return 0.5; }
    }
    if (az >= ax) { return 0.8; }
    return 0.6;
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.uv = in.uv;

    // Unpack light nibbles: bits 0..=3 block, 4..=7 sky
    let block_nibble = f32(in.light & 0x0Fu) / 15.0;
    let sky_nibble = f32((in.light >> 4u) & 0x0Fu) / 15.0;

    let block_light = light_brightness(block_nibble);
    let sky_light = light_brightness(sky_nibble) * camera.sun_brightness;
    out.light = gamma_correct(clamp(block_light + sky_light, 0.05, 1.0));
    out.shade = face_shade(in.normal);
    // Bit 8: hurt tint. Bits 9..=12: white flash level (the creeper fuse).
    // Bit 13: dye tint, bits 14..=17 the dye.
    out.hurt = f32((in.light >> 8u) & 1u);
    out.flash = f32((in.light >> 9u) & 15u) / 15.0;
    let dyed = (in.light >> 13u) & 1u;
    out.dye = select(vec3<f32>(1.0), DYES[(in.light >> 14u) & 15u], dyed == 1u);

    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(entity_texture, entity_sampler, in.uv);
    if (tex.a < 0.1) {
        discard;
    }

    // A hurt or dying entity is blended 30 % towards red before the light is
    // applied (MCP `RendererLivingEntity.setBrightness`: (1, 0, 0, 0.3)).
    var base = mix(tex.rgb * in.dye, vec3<f32>(1.0, 0.0, 0.0), 0.3 * in.hurt);
    // The creeper about to explode flashes white, at most 20 %
    // (MCP `RenderCreeper.getColorMultiplier`).
    base = mix(base, vec3<f32>(1.0), 0.2 * in.flash);
    let lit = base * in.light * in.shade;
    let dist = distance(in.world_position, camera.camera_pos);
    let fog = clamp((dist - camera.fog_start) / (camera.fog_end - camera.fog_start), 0.0, 1.0);
    let final_rgb = mix(lit, camera.fog_color, fog);

    return vec4<f32>(final_rgb, tex.a);
}
