# M6-W3 — sky, sun/moon/clouds and the world clock

Owner worktree: `/home/mathip/dev/rust/mc-project/.worktrees/m6-sky`
Branch: `feat/m6-sky`
Depends on: M6 setup on `main` (ADR-0008)

## Goal

Replace the placeholder `SkyState`/`SkyRenderer` with the real time-of-day
model and an original GPU sky pass (sky gradient, sun, moon, stars and
procedural clouds). Carry the server's `Time Update` packet to the render
thread so the live client follows the world clock.

## Owned files

- `crates/umicraft-render/src/sky.rs`
- `crates/umicraft-render/src/sky.wgsl` (new)
- `crates/umicraft-client/src/game.rs`
- `crates/umicraft-client/src/network.rs`
- `crates/umicraft-client/src/window.rs`

Do **not** edit `renderer.rs`, `terrain.wgsl`, `environment.rs`, `lib.rs`,
`block_demo.rs`, `args.rs`, `screenshot.rs`, world/mesh, or docs. The
`SkyRenderer` hooks and `SkyState` fields are frozen; W2 calls them.

## Required behaviour

### SkyState (pure, testable)

Implement `SkyState::at(environment: RenderEnvironment)` following the 1.8
curves cited in ADR-0008:

1. `celestial_angle` for `world_time` (ticks) + `partial_tick`:
   - `i = world_time.rem_euclid(24_000)` (negative frozen times stay defined);
   - `f = (i + partial) / 24_000 - 0.25`, wrapped to `0..1`;
   - `f = 1 - (cos(f * PI) + 1) / 2`;
   - this yields sunrise around 0 and sunset around 0.5.
2. `sun_brightness = clamp(1 - (cos(angle * 2PI) * 2 + 0.2), 0, 1)`, then
   `* 0.8 + 0.2`; rain/thunder are zero in M6.
3. `sky_color`: the 1.8 `getSkyColor` shape with a fixed temperate biome
   colour (`0x78A7FF` is a good original default). Use
   `f1 = clamp(cos(angle * 2PI) * 2 + 0.5, 0, 1)` and multiply the base RGB by
   `f1`. Keep it in linear space.
4. `fog_color`: the `WorldProvider.getFogColor` formula:
   `f = clamp(cos(angle * 2PI) * 2 + 0.5, 0, 1)`,
   `rgb = [0.7529412, 0.84705883, 1.0] * [f*0.94+0.06, f*0.94+0.06, f*0.91+0.09]`.
5. `cloud_color`: white base `[1.0, 1.0, 1.0]` modulated by
   `f1 * 0.9 + 0.1` (blue slightly less), alpha `0.8`.
6. `star_alpha = clamp(1 - (cos(angle * 2PI) * 2 + 0.25), 0, 1)^2 * 0.5`.
7. `moon_phase = (world_time.div_euclid(24_000).rem_euclid(8)) as u8`.
8. Directions: sun and moon opposite on the celestial circle, normalized
   `[f32; 3]`. The sky shader only needs stable directions; document the
   convention in the module comment.
9. Dimensions: for `dimension != 0`, return a dark dimension-specific sky and
   no sun/stars/clouds (the Nether/End are not the M6 visual target, but they
   must not show an overworld noon).

Keep the placeholder colours only until the real curves are in place; remove
the placeholder test.

### SkyRenderer (GPU)

- Own a wgpu pipeline, uniform buffer (camera inverse view-projection, camera
  position, `SkyState` fields, cloud offset), bind group and any static
  geometry. `SkyRenderer::new(device, format)` must use
  `crate::renderer::DEPTH_FORMAT` (crate-private, already exported) for the
  depth-stencil state.
- Draw one fullscreen triangle or a far-plane cube. Depth compare `Always`,
  depth write off, no culling. W2 draws it at the start of the existing main
  pass, so do not create a render pass or clear here.
- Reconstruct the view ray in the fragment shader from the inverse
  view-projection. Render:
  - a vertical sky gradient from `sky_color` at the horizon to a darker zenith;
  - a procedural sun disc (original, no texture) with a soft edge, visible
    when `sun_direction` is above the horizon;
  - a procedural moon disc with a phase mask from `moon_phase`;
  - procedural stars hashed from the ray direction, alpha `star_alpha`, only
    in the upper hemisphere;
  - a cloud layer at y = 128: intersect the ray with the plane, hash/noise the
    world XZ position, draw white `cloud_color` puffs with a soft edge. The
    layer must be static in world space and move with `world_time`; no Mojang
    cloud texture.
- Clamp the ray-plane intersection and avoid NaNs for rays parallel to the
  cloud plane. Keep the shader branch-light and deterministic.
- `SkyRenderer::update` writes the uniform. `SkyRenderer::draw` submits the
  pipeline. Both are no-ops only before W3 lands; the final versions must
  render.

### Client clock

- `umicraft-protocol` already decodes `TimeUpdate`; add
  `NetEvent::TimeUpdate(TimeUpdate)` to `network.rs` and dispatch 0x03.
- `Game` stores `world_time: i64` (signed `time_of_day`, preserving the frozen
  sign). Reset it on `JoinGame` and on a dimension-changing `Respawn`; keep it
  on a same-dimension respawn.
- `Frame` gains `world_time: i64` and `dimension: i8` as frozen in ADR-0008.
- The window `View` stores the newest world time/dimension alongside the
  interpolated state. `App` builds
  `RenderEnvironment::new(dimension, world_time, partial_tick, args.view_distance)`
  once per frame and calls `Renderer::set_environment` before rendering. Use
  the same interpolated tick fraction the camera uses.
- Do not advance the clock locally between packets; the 1.8 server sends
  `Time Update` regularly. A frozen negative time stays frozen.
- Preserve `Frame`'s `Copy` derive and the existing frame-channel capacity and
  non-blocking publish path.

## Tests

- `SkyState` at times 0/6000/12000/18000: sun brightness, sky/fog colour
  ordering, star alpha, and moon phase are plausible and finite.
- Negative `world_time` uses Euclidean modulo and does not panic.
- Nether/End state is visibly different and has zero star/sun brightness.
- A `Time Update` decode reaches the game state (unit test on the dispatch or
  a small `Game` helper; do not start a network thread).
- The frame carries the time/dimension and the render environment clamps the
  partial tick.
- A shader compilation smoke test is covered by the existing ignored offscreen
  smoke; add an ignored `offscreen_sky` test only if the pipeline can be
  tested without duplicating W2's renderer.

## Gates

```sh
cargo fmt -p umicraft-render -p umicraft-client -- --check
cargo clippy -p umicraft-render -p umicraft-client --all-targets -- -D warnings
cargo test -p umicraft-render -p umicraft-client
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-render -p umicraft-client --no-deps
cargo test -p umicraft-render renderer::tests::offscreen_smoke_renders_sky_and_overlay -- --ignored
```

Report the commit SHA, test count, the GPU adapter used and any change needed
in W2's renderer. Do not merge.
