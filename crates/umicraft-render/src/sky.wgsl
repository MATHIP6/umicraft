// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Original procedural sky pass (M6): a vertical gradient, a sun disc, a
// phased moon disc, a hashed star field and a cloud layer at y = 128.
// No Mojang texture or shader is used (spec §3).
//
// The vertex stage emits one fullscreen triangle. The fragment stage
// reconstructs the world-space view direction from the inverse
// view-projection and evaluates the sky analytically.

struct SkyUniforms {
    inverse_view_projection: mat4x4<f32>,
    // xyz eye position, w unused.
    camera_position: vec4<f32>,
    // xyz sun direction, w celestial visibility (1 in the overworld).
    sun_direction: vec4<f32>,
    // xyz moon direction, w moon phase in 0..7.
    moon_direction: vec4<f32>,
    // rgb horizon colour, a star alpha.
    sky_horizon: vec4<f32>,
    // rgb zenith colour, a sun brightness.
    sky_zenith: vec4<f32>,
    // rgb fog colour, w unused.
    fog_color: vec4<f32>,
    // rgba cloud tint.
    cloud_color: vec4<f32>,
    // xy cloud drift, z cloud height, w unused.
    cloud_offset: vec4<f32>,
};

@group(0) @binding(0) var<uniform> sky: SkyUniforms;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    // A single triangle that covers the whole clip-space square.
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let ndc = corners[index];
    var out: VertexOut;
    // The depth compare is `Always` and depth writes are off; z = 1 keeps
    // the triangle on the far plane.
    out.position = vec4<f32>(ndc, 1.0, 1.0);
    out.ndc = ndc;
    return out;
}

// Stable hash in 0..1 from a 2D cell. The constants are the usual
// sine-free integer mixing constants; the result is deterministic per cell.
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + vec3<f32>(dot(p3, p3.yzx + vec3<f32>(33.33)));
    return fract((p3.x + p3.y) * p3.z);
}

// Bilinear value noise with a smoothstep interpolation.
fn value_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(cell);
    let b = hash21(cell + vec2<f32>(1.0, 0.0));
    let c = hash21(cell + vec2<f32>(0.0, 1.0));
    let d = hash21(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Four octaves of value noise; the loop bound is constant and deterministic.
fn fbm(p: vec2<f32>) -> f32 {
    var total = 0.0;
    var amplitude = 0.55;
    var sample_point = p;
    for (var octave = 0; octave < 4; octave = octave + 1) {
        total = total + amplitude * value_noise(sample_point);
        sample_point = sample_point * 2.03 + vec2<f32>(17.0, 31.0);
        amplitude = amplitude * 0.5;
    }
    return total;
}

// Additive star brightness for one ray. The stars live on a spherical grid
// so they do not stretch at the zenith, and only the upper hemisphere is lit.
fn star_field(direction: vec3<f32>, alpha: f32) -> f32 {
    if (alpha <= 0.0 || direction.y <= 0.0) {
        return 0.0;
    }
    let elevation = asin(clamp(direction.y, -1.0, 1.0));
    let azimuth = atan2(direction.z, direction.x);
    let grid = vec2<f32>(azimuth * 48.0, elevation * 48.0);
    let cell = floor(grid);
    let cell_offset = fract(grid) - vec2<f32>(0.5);
    let random = hash21(cell);
    let shape = 1.0 - smoothstep(0.0, 0.38, length(cell_offset));
    let brightness = 0.35 + 0.65 * hash21(cell + vec2<f32>(11.0, 7.0));
    let hemisphere = smoothstep(0.0, 0.2, direction.y);
    return step(0.982, random) * shape * brightness * alpha * hemisphere;
}

// Sun disc plus a soft glow. `w` of the direction carries the overworld
// visibility so the Nether/End never show a sun.
fn sun_contribution(direction: vec3<f32>) -> f32 {
    let sun = normalize(sky.sun_direction.xyz);
    let alignment = dot(direction, sun);
    let disc = smoothstep(0.99820, 0.99875, alignment);
    let glow = pow(max(alignment, 0.0), 180.0) * 0.35;
    let above_horizon = smoothstep(-0.03, 0.03, sky.sun_direction.y);
    return clamp((disc + glow) * above_horizon * sky.sun_direction.w, 0.0, 1.0);
}

// Lit part of the moon disc. The phase is a mask: a shadow disc the same
// size as the moon slides across it, which gives the full -> new -> full
// crescent cycle without a texture. Phase 0 is full (the shadow is two
// radii away), phase 4 is new (the shadow is centred on the disc).
fn moon_contribution(direction: vec3<f32>) -> f32 {
    let moon = normalize(sky.moon_direction.xyz);
    let alignment = dot(direction, moon);
    if (alignment <= 0.0) {
        return 0.0;
    }
    let projected = direction - moon * alignment;
    // `moon` lies in the XY plane, so world Z is a valid tangent and the
    // cross product gives the other one.
    let tangent_x = normalize(cross(moon, vec3<f32>(0.0, 0.0, 1.0)));
    // The disc radius is about 3.4 degrees (sin(0.06)); `u` is in units of
    // that radius, so the edge is length(u) == 1.
    let u = vec2<f32>(
        dot(projected, tangent_x),
        dot(projected, vec3<f32>(0.0, 0.0, 1.0)),
    ) / 0.06;
    let disc = 1.0 - smoothstep(0.94, 1.0, length(u));

    let phase = sky.moon_direction.w;
    let fullness = 0.5 + 0.5 * cos(phase * 0.7853981634);
    let side = select(-1.0, 1.0, phase < 4.0);
    let shadow_center = vec2<f32>(side * 2.0 * fullness, 0.0);
    let shadow = 1.0 - smoothstep(0.9, 1.0, length(u - shadow_center));

    let above_horizon = smoothstep(-0.03, 0.03, sky.moon_direction.y);
    return clamp(disc * (1.0 - shadow) * above_horizon * sky.sun_direction.w, 0.0, 1.0);
}

// Cloud coverage for one ray. The layer is a hash/noise field anchored to the
// world XZ position plus the clock drift; rays parallel to the plane return
// zero instead of a NaN. The intersection is clamped to the distance where
// the layer has already faded out, so grazing rays keep a bounded hash input.
const MAX_CLOUD_DISTANCE: f32 = 512.0;

fn cloud_contribution(direction: vec3<f32>) -> f32 {
    if (abs(direction.y) < 1e-4) {
        return 0.0;
    }
    let distance_to_plane = (sky.cloud_offset.z - sky.camera_position.y) / direction.y;
    if (distance_to_plane <= 0.0) {
        return 0.0;
    }
    let distance = min(distance_to_plane, MAX_CLOUD_DISTANCE);
    let world = sky.camera_position.xz + direction.xz * distance + sky.cloud_offset.xy;
    let density = fbm(world / 90.0);
    let mask = smoothstep(0.52, 0.68, density);
    let distance_fade = 1.0 - smoothstep(260.0, 480.0, distance_to_plane);
    let horizon_fade = smoothstep(0.0, 0.12, abs(direction.y));
    return clamp(mask * distance_fade * horizon_fade * sky.cloud_color.a, 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Reconstruct the view ray from the inverse view-projection. The near
    // and far points are both finite for a valid perspective projection.
    let near_h = sky.inverse_view_projection * vec4<f32>(in.ndc, 0.0, 1.0);
    let far_h = sky.inverse_view_projection * vec4<f32>(in.ndc, 1.0, 1.0);
    let direction = normalize(far_h.xyz / far_h.w - near_h.xyz / near_h.w);

    let up = clamp(direction.y, 0.0, 1.0);
    var color = mix(sky.sky_horizon.rgb, sky.sky_zenith.rgb, smoothstep(0.0, 0.65, up));
    // The horizon fades into the fog colour so the terrain fog and the sky
    // agree at the silhouette.
    color = mix(sky.fog_color.rgb, color, smoothstep(0.0, 0.10, up));

    let sun = sun_contribution(direction);
    let sun_color = vec3<f32>(1.0, 0.93, 0.78) * (0.6 + 0.4 * sky.sky_zenith.w);
    color = mix(color, sun_color, sun);

    let moon = moon_contribution(direction);
    color = mix(color, vec3<f32>(0.82, 0.87, 1.0), moon);

    color = color + vec3<f32>(0.85, 0.9, 1.0) * star_field(direction, sky.sky_horizon.w);

    let clouds = cloud_contribution(direction);
    color = mix(color, sky.cloud_color.rgb, clouds);

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
