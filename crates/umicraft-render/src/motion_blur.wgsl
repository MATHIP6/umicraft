// SPDX-License-Identifier: MIT OR Apache-2.0

// Motion blur of the rendered world (the "Motion Blur" mod), three versions:
//
// * `fs_accum` (V1): the classic one. The picture is mixed with the previous
//   output, so what moved leaves a trail that fades.
// * `fs_v2` (V2): the depth of each pixel and the camera of the previous frame
//   give where that pixel was a frame ago; the picture is averaged along that
//   line, ten taps with a jitter that hides the banding.
// * `fs_v3` (V3): the same line, with a number of taps that follows its length
//   and a tent weight, and a pixel that is much closer than the one being blurred
//   counts for little, so an edge does not smear over what is behind it.
//
// The hand, the overlays and the HUD are drawn after this, sharp. In the third
// person the body of the player is part of the picture but moves with the camera,
// not with the world, so a capsule around it is left unblurred.

struct Params {
    // The inverse of this frame's view-projection and the previous frame's
    // view-projection, both relative to the same render origin.
    inverse_view_projection: mat4x4<f32>,
    previous_view_projection: mat4x4<f32>,
    // x: share of the previous picture kept (V1) or how much of the motion of the
    //    frame the blur covers (V2, V3); y: the longest blur, in screen heights;
    // z: strength; w: seed of the jitter, 0 to 1.
    a: vec4<f32>,
    // x: aspect ratio (width / height); y: near plane; z: far plane; w: the depth
    // below which a pixel is the hand.
    b: vec4<f32>,
    // The body of the player, as a vertical capsule relative to the render
    // origin: xyz of the feet and of the head, and in w of the feet the radius, or
    // zero when no body is in view (the first person). The w of the head is the
    // share of the previous picture the hand keeps.
    body_feet: vec4<f32>,
    body_head: vec4<f32>,
};

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var history: texture_2d<f32>;
@group(0) @binding(4) var<uniform> p: Params;

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

// Distance from the camera of a point of this depth.
fn view_distance(d: f32) -> f32 {
    let near = p.b.y;
    let far = p.b.z;
    return near * far / (far - d * (far - near));
}

// The point of the world, relative to the render origin, seen at `uv` with depth `d`.
fn world_point(uv: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let world = p.inverse_view_projection * vec4<f32>(ndc, d, 1.0);
    return world.xyz / world.w;
}

// How much `point` is part of the body of the player: `1.0` well inside the
// capsule around it, fading to `0.0` at its radius, `0.0` with no body in view.
fn body_mask(point: vec3<f32>) -> f32 {
    let radius = p.body_feet.w;
    if (radius <= 0.0) {
        return 0.0;
    }
    let axis = p.body_head.xyz - p.body_feet.xyz;
    let along = clamp(dot(point - p.body_feet.xyz, axis) / dot(axis, axis), 0.0, 1.0);
    let away = length(point - (p.body_feet.xyz + axis * along));
    return 1.0 - smoothstep(0.75 * radius, radius, away);
}

// How far the point `point` seen at `uv` has moved on the screen since the
// previous frame, in texture coordinates. Zero when it was behind that camera, and
// for the body of the player, which moves with the camera.
fn screen_velocity(uv: vec2<f32>, point: vec3<f32>) -> vec2<f32> {
    let before = p.previous_view_projection * vec4<f32>(point, 1.0);
    if (before.w <= 0.0001) {
        return vec2<f32>(0.0);
    }
    let before_ndc = before.xy / before.w;
    let before_uv = vec2<f32>(before_ndc.x * 0.5 + 0.5, 0.5 - before_ndc.y * 0.5);
    return (uv - before_uv) * (1.0 - body_mask(point));
}

// Shortens a blur that is longer than the limit; the length is measured in screen
// heights, so it does not depend on the shape of the window.
fn limited(v: vec2<f32>) -> vec2<f32> {
    let tall = length(vec2<f32>(v.x * p.b.x, v.y));
    if (tall > p.a.y) {
        return v * (p.a.y / tall);
    }
    return v;
}

// Interleaved gradient noise: a different offset for each pixel and frame.
fn noise(pixel: vec2<f32>) -> f32 {
    let shifted = pixel + p.a.w * 47.0;
    return fract(52.9829189 * fract(dot(shifted, vec2<f32>(0.06711056, 0.00583715))));
}

fn sharp(uv: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(textureSampleLevel(scene, samp, uv, 0.0).rgb, 1.0);
}

// The hand moves with the camera, so the motion of the world says nothing about
// it: where it is, or where it just was, the picture is mixed with the previous
// one instead, and what it leaves behind fades out. The alpha of the picture
// keeps how much of the hand is left in a pixel, for the next frame.
fn with_hand(base: vec3<f32>, uv: vec2<f32>, d: f32) -> vec4<f32> {
    // A hand that stands still keeps no trail, and reads nothing more.
    if (p.body_head.w <= 0.0) {
        return vec4<f32>(base, 0.0);
    }
    let before = textureSampleLevel(history, samp, uv, 0.0);
    let on_hand = select(0.0, 1.0, d < p.b.w);
    let weight = max(on_hand, before.a);
    let rgb = mix(base, before.rgb, p.body_head.w * weight);
    return vec4<f32>(rgb, max(on_hand, before.a * p.body_head.w));
}

@fragment
fn fs_accum(in: VertexOut) -> @location(0) vec4<f32> {
    let now = textureSampleLevel(scene, samp, in.uv, 0.0).rgb;
    let before = textureSampleLevel(history, samp, in.uv, 0.0).rgb;
    // The body of the player keeps no trail.
    let point = world_point(in.uv, textureLoad(depth, vec2<i32>(in.position.xy), 0));
    let kept = p.a.x * (1.0 - body_mask(point));
    // V1 mixes the whole picture; its alpha is not a hand trail.
    return vec4<f32>(mix(now, before, kept), 0.0);
}

const V2_TAPS: i32 = 10;

@fragment
fn fs_v2(in: VertexOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let d = textureLoad(depth, pixel, 0);
    let size = vec2<f32>(textureDimensions(scene));
    var v = limited(screen_velocity(in.uv, world_point(in.uv, d)) * p.a.x);
    if (d < p.b.w) {
        v = vec2<f32>(0.0);
    }
    if (length(v * size) < 0.75) {
        return with_hand(sharp(in.uv).rgb, in.uv, d);
    }
    let shift = noise(in.position.xy) - 0.5;
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < V2_TAPS; i = i + 1) {
        let t = (f32(i) + 0.5 + shift) / f32(V2_TAPS) - 0.5;
        sum += textureSampleLevel(scene, samp, in.uv + v * t, 0.0).rgb;
    }
    return with_hand(sum / f32(V2_TAPS), in.uv, d);
}

@fragment
fn fs_v3(in: VertexOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let d = textureLoad(depth, pixel, 0);
    let size = vec2<f32>(textureDimensions(scene));
    var v = limited(screen_velocity(in.uv, world_point(in.uv, d)) * p.a.x);
    if (d < p.b.w) {
        v = vec2<f32>(0.0);
    }
    let length_px = length(v * size);
    if (length_px < 0.75) {
        return with_hand(sharp(in.uv).rgb, in.uv, d);
    }
    // About one tap every two pixels of the blur, between four and sixteen.
    let taps = clamp(i32(ceil(length_px / 2.0)), 4, 16);
    let centre = view_distance(d);
    let tolerance = 0.05 * centre + 0.25;
    let shift = noise(in.position.xy) - 0.5;
    let last = size - vec2<f32>(1.0);
    var sum = textureSampleLevel(scene, samp, in.uv, 0.0).rgb;
    var weight = 1.0;
    for (var i = 0; i < taps; i = i + 1) {
        let t = (f32(i) + 0.5 + shift) / f32(taps) - 0.5;
        let at = in.uv + v * t;
        let tent = 1.0 - 0.6 * abs(2.0 * t);
        // A tap much closer than this pixel is something else in front of it.
        let tap_pixel = vec2<i32>(clamp(at * size, vec2<f32>(0.0), last));
        let closer = centre - view_distance(textureLoad(depth, tap_pixel, 0));
        let in_front = smoothstep(tolerance, 2.0 * tolerance, closer);
        let w = tent * (1.0 - 0.85 * in_front);
        sum += textureSampleLevel(scene, samp, at, 0.0).rgb * w;
        weight += w;
    }
    return with_hand(sum / weight, in.uv, d);
}
