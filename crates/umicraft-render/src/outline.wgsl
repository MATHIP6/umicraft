// SPDX-License-Identifier: MIT OR Apache-2.0

// Wireframe lines (selection outline, debug boxes, the lines of mods): each edge
// is an instance of a quad that is widened in screen space, because wgpu only
// draws one-pixel lines. The colour and the width in pixels come with the edge;
// the colour is alpha-blended by the pipeline.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_brightness: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    gamma: f32,
    fog_density: f32,
    _pad1: f32,
    // Integer world offset of the render origin: positions are sent to the
    // GPU relative to it (`view_proj` is built for that origin), so f32
    // keeps its precision far from the world origin.
    origin: vec3<f32>,
};

struct Screen {
    // x, y: viewport size in pixels.
    values: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> screen: Screen;

// Distance in front of the camera plane below which a point is clipped.
const NEAR_W: f32 = 0.05;
// Pulls the line towards the camera so it wins against the block faces.
const DEPTH_BIAS: f32 = 0.0001;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) corner: u32,
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) width: f32,
) -> VertexOut {
    var a = camera.view_proj * vec4<f32>(start - camera.origin, 1.0);
    var b = camera.view_proj * vec4<f32>(end - camera.origin, 1.0);
    // An edge behind the camera is not drawn; a crossing one is cut at the
    // near distance so the projection stays finite.
    if (a.w < NEAR_W && b.w < NEAR_W) {
        return VertexOut(vec4<f32>(2.0, 2.0, 2.0, 1.0), color);
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
    let offset = normal * side * width / viewport;
    return VertexOut(vec4<f32>(base.xy + offset, max(base.z - DEPTH_BIAS, 0.0), 1.0), color);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}
