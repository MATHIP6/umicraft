// SPDX-License-Identifier: MIT OR Apache-2.0

// Block selection outline: each edge is an instance of a quad that is widened
// in screen space, because wgpu only draws one-pixel lines. The colour is
// vanilla's (0, 0, 0, 0.4), alpha-blended by the pipeline.

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

struct Screen {
    // x, y: viewport size in pixels; z: line width in pixels.
    values: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> screen: Screen;

// Distance in front of the camera plane below which a point is clipped.
const NEAR_W: f32 = 0.05;
// Pulls the line towards the camera so it wins against the block faces.
const DEPTH_BIAS: f32 = 0.0001;

@vertex
fn vs_main(
    @builtin(vertex_index) corner: u32,
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
) -> @builtin(position) vec4<f32> {
    var a = camera.view_proj * vec4<f32>(start, 1.0);
    var b = camera.view_proj * vec4<f32>(end, 1.0);
    // An edge behind the camera is not drawn; a crossing one is cut at the
    // near distance so the projection stays finite.
    if (a.w < NEAR_W && b.w < NEAR_W) {
        return vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    if (a.w < NEAR_W) {
        a = mix(a, b, (NEAR_W - a.w) / (b.w - a.w));
    } else if (b.w < NEAR_W) {
        b = mix(b, a, (NEAR_W - b.w) / (a.w - b.w));
    }
    let viewport = screen.values.xy;
    let pa = a.xyz / a.w;
    let pb = b.xyz / b.w;
    let along = (pb.xy - pa.xy) * viewport;
    var dir = vec2<f32>(1.0, 0.0);
    if (length(along) > 0.0001) {
        dir = normalize(along);
    }
    let normal = vec2<f32>(-dir.y, dir.x);

    // Two triangles: (end, side) = (0,-1) (0,+1) (1,-1) (1,-1) (0,+1) (1,+1).
    let at_end = corner == 2u || corner == 3u || corner == 5u;
    let side = select(1.0, -1.0, corner == 0u || corner == 2u || corner == 3u);
    let base = select(pa, pb, at_end);
    let offset = normal * side * screen.values.z / viewport;
    return vec4<f32>(base.xy + offset, max(base.z - DEPTH_BIAS, 0.0), 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.4);
}
