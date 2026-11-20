# Post-process passes

API 0.6, F3.7, ADR-0086. A mod adds full-screen passes over the picture of the
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
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;

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

The source is checked before it is accepted, with naga and every validation flag:
at most 64 KiB, it must parse and validate, the module must have exactly the entry
points `umi_vs` and `fs_main`, and it may declare no resource beyond the three of the
prelude. A refused shader makes `post_pass` fail with the naga message in the log. A
mod has at most 8 passes. A pass the GPU still refuses when the client compiles it is
switched off for the run, its mod keeps running.

A pass costs GPU time on every frame it runs; v1 does not measure it per mod yet
(F3.8). Switching the mod off stops its passes on the next frame.
