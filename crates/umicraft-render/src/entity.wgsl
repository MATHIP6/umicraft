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
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
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
};

fn light_brightness(level: f32) -> f32 {
    return level / (3.0 * (1.0 - level) + 1.0);
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
    out.light = clamp(block_light + sky_light, 0.05, 1.0);
    out.shade = face_shade(in.normal);

    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(entity_texture, entity_sampler, in.uv);
    if (tex.a < 0.1) {
        discard;
    }

    let lit = tex.rgb * in.light * in.shade;
    let dist = distance(in.world_position, camera.camera_pos);
    let fog = clamp((dist - camera.fog_start) / (camera.fog_end - camera.fog_start), 0.0, 1.0);
    let final_rgb = mix(lit, camera.fog_color, fog);

    return vec4<f32>(final_rgb, tex.a);
}
