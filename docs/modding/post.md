# Post-process passes

API 0.6, F3.7, ADR-0086; scene depth since API 1.1 (ADR-0094). A mod adds full-screen passes over the picture of the
world: colour grading, a vignette, a blur. No permission is needed: a pass sees only
the picture it is given.

## Where a pass runs

After the world and the hand are drawn, before the motion blur, the menu blur, the
screen overlays (vignette, pumpkin, portal) and the HUD, which stay sharp. The passes
run in load order, then in the order each mod registered them; each reads what the
previous one wrote. With no enabled pass the frame is drawn exactly as without mods.

## Writing one

`cx.post_pass("sepia", "Sepia", include_str!("sepia.wgsl"))` (C: `register_post_pass`)
during `init`. The host puts this prelude in front of the source:

```wgsl
struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
    depth: vec4<f32>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;
@group(0) @binding(3) var umi_depth: texture_depth_2d;

struct UmiVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn umi_vs(@builtin(vertex_index) index: u32) -> UmiVertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: UmiVertexOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn umi_linear_depth(uv: vec2<f32>) -> f32 {
    if umi.depth.z < 0.5 {
        return umi.depth.y;
    }
    let size = vec2<i32>(textureDimensions(umi_depth));
    let texel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
    let d = textureLoad(umi_depth, texel, 0);
    return umi.depth.x * umi.depth.y / (umi.depth.y - d * (umi.depth.y - umi.depth.x));
}
```

The mod writes only `fs_main` and its helpers:

```wgsl
@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    let scene = textureSample(umi_scene, umi_sampler, in.uv);
    let grey = dot(scene.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(mix(scene.rgb, vec3<f32>(grey), umi.params[0].x), scene.a);
}
```

- `uv` runs 0..1, y down; the sampler is linear and clamps to the edge.
- `umi.resolution` is in pixels, `umi.time` in seconds since the client started,
  `umi.frame` counts frames.
- `umi.params` are four `vec4` the mod sets from any thread with
  `pass.set_param(index, [x, y, z, w])` (C: `post_set_param`); zero until set. Settings
  have no change event, so a mod pushes them from a tick handler, as
  `mods/examples/shader_pass` does.

## Scene depth (API 1.1)

`umi_depth` (binding 3) is the depth buffer of the world, and `umi_linear_depth(uv)`
turns it into the distance in **blocks along the view axis** (not along the ray, so
it is the same across a flat wall facing you). The sky reads the far plane, 512
blocks. `umi.depth` is `(near, far, valid, 0)`: near 0.1 and far 512 are the
projection planes the helper inverts, and `valid` is 1.0 when `umi_depth` is the
real scene depth and 0.0 when the host bound a 1x1 placeholder (the helper then
returns far for every pixel).

- The depth is read with `textureLoad` at the texel under `uv`, never through the
  sampler: a depth texture cannot be sampled with a filtering sampler, so there is no
  smoothing between texels. Read `umi_depth` yourself with `textureLoad` if you need
  the raw 0..1 value (not reversed: 0 near, 1 far).
- Hand: the first-person hand is drawn into the first 1e-4 of the depth range so it
  always wins the depth test; its pixels therefore read as very near (about the near
  plane) and a distance effect never covers the hand.
- Depth is that of the world pass, before any pass runs. A pass that writes colour
  does not change it, so every pass of the chain sees the same depth.
- The depth texture is only sampled while the passes run: the world pass that writes
  it has ended, and a post pass has no depth attachment.
- A shader written for API 1.0 never mentions depth and runs unchanged; the
  `UmiPost` struct only grew at its end (`depth` at offset 80, 96 bytes in all).

`mods/examples/depth_fog` fades pixels beyond a distance toward a fog colour.

The source is checked before it is accepted, with naga and every validation flag:
at most 64 KiB, it must parse and validate, the module must have exactly the entry
points `umi_vs` and `fs_main`, and it may declare no resource beyond the four of the
prelude: the resource whitelist is the four bindings of the prelude, `umi_scene`,
`umi_sampler`, `umi` and `umi_depth`. A refused shader makes `post_pass` fail with the naga message in the log. A
mod has at most 8 passes. A pass the GPU still refuses when the client compiles it is
switched off for the run, its mod keeps running.

A pass costs GPU time on every frame it runs; v1 does not measure it per mod yet
(F3.8). Switching the mod off stops its passes on the next frame.
