# M6-W2 — terrain lighting, transparency and fog

Owner worktree: `/home/mathip/dev/rust/mc-project/.worktrees/m6-render`
Branch: `feat/m6-render`
Depends on: M6 setup on `main` (ADR-0008)

## Goal

Consume the frozen per-vertex light/AO payload in the terrain shader, split the
GPU mesh into opaque and transparent buffers, add the back-to-front transparent
pass with blending, and drive the fog and sky clear colour from the frozen
`RenderEnvironment`/`SkyState` API.

## Owned files

- `crates/umicraft-render/src/renderer.rs`
- `crates/umicraft-render/src/terrain.wgsl`
- `crates/umicraft-render/src/transparency.rs` (new, if useful)
- `crates/umicraft-render/tests/offscreen_terrain.rs` (new)
- inline `#[cfg(test)]` tests in `renderer.rs` / `transparency.rs`

Do **not** edit `crates/umicraft-render/src/sky.rs`, `sky.wgsl`, `lib.rs`,
`environment.rs`, any other crate, or docs. `SkyState` and `SkyRenderer` are
owned by W3; consume their frozen fields and hooks. If a signature is missing,
stop and report it rather than editing `sky.rs`.

## Required behaviour

1. Keep `Renderer::set_environment` and call `SkyRenderer::update`/`draw`
   exactly at the lifecycle points already scaffolded: update once per frame
   with the current camera and environment; draw at the start of the main
   render pass before terrain. The no-op body in `sky.rs` will be replaced by
   W3, so W2 must not depend on its internals.
2. The terrain vertex layout already has a fifth `Uint32` attribute at shader
   location 4. Unpack it in `terrain.wgsl`:
   - bits 0..=3 block light;
   - bits 4..=7 sky light;
   - bits 8..=11 AO;
   - bit 12 `shade`.
3. Lighting model:
   - `b(level) = (level/15) / (3 * (1 - level/15) + 1)`;
   - sky contribution `b(sky) * sun_brightness`;
   - block contribution `b(block)`;
   - `light = min(1.0, sky + block)`;
   - AO factor `0.2 + 0.8 * (ao/15)` (15 is open, 0 is the darkest corner);
   - axis face shading only when the `shade` bit is set; otherwise the shade
     factor is `1.0` (crosses, plants, torches);
   - multiply the sampled texture RGB by `light * ao * shade`.
4. Transparent pass:
   - extend `GpuMesh` with separate opaque and transparent vertex/index
     buffers and counts; upload both halves in `upload_mesh`; remove the mesh
     only when both are empty;
   - opaque pipeline: depth `Less`, depth write on, `REPLACE`, alpha cutout
     (`discard` when `a < 0.5`);
   - transparent pipeline: depth `LessEqual`, depth write **off**, blend
     `SrcAlpha`/`OneMinusSrcAlpha`, no alpha cutout (water alpha is real);
   - draw opaque chunks first, then transparent chunks sorted farthest first
     by squared distance from `camera.position` to the chunk centre
     `(pos.x*16+8, 128, pos.z*16+8)`. Tie-break by `ChunkPos` so frames are
     deterministic. Reuse one bounded `Vec` owned by the renderer; do not
     allocate or rebuild GPU buffers per frame;
   - the transparent shader may live in `terrain.wgsl` as a second fragment
     entry point or in a new file, but it must share the vertex entry point.
5. Fog:
   - `fog_start = view_distance as f32 * 16.0 * 0.75`;
   - `fog_end = (view_distance as f32 * 16.0).min(500.0)`;
   - use `SkyState::at(environment).fog_color` as the target;
   - darken the fog target by the fragment light so a cave does not show a
     bright blue haze: `fog_color * (0.15 + 0.85 * combined_light)` is a valid
     starting point; keep the surface look bright;
   - use `smoothstep(fog_start, fog_end, distance)`.
6. Update `CameraUniform` (or add a second small uniform) with
   `sun_brightness`, `fog_color`, `fog_start`, `fog_end`; keep it `Pod` and
   16-byte aligned. `write_camera` may take the environment or the computed
   `SkyState`.
7. Keep the fallback path working: a mesh with only `vertices`/`indices`
   (e.g. M2 synthetic tests) renders exactly as before except for full-bright
   lighting. The procedural texture array and alpha cutout remain.
8. The clear colour becomes `SkyState::at(environment).sky_color`; the sky
   pass draws over it. Do not add a separate render pass for sky: W3's
   `SkyRenderer::draw` is called inside the existing main pass.
9. GPU tests can be `#[ignore]` like the existing offscreen tests. Unit-test
   the sort function without a GPU:
   - farthest distance first;
   - equal distances tie-break by `ChunkPos`;
   - an empty list is a no-op.

## Tests

- `cargo test -p umicraft-render` (non-GPU) must pass.
- Add an ignored `tests/offscreen_terrain.rs` with at least:
  - a daytime frame and a night `RenderEnvironment` differ in average terrain
    brightness (night darker);
  - a transparent water quad over an opaque coloured surface blends instead
    of replacing it;
  - the transparent pass does not hide an opaque quad drawn after it in
    upload order when the opaque quad is nearer (depth test still applies);
  - a cave-like environment/full-dark vertex does not paint bright sky fog.
  Use synthetic meshes and the fallback texture array; no official assets.
- The existing `offscreen_ui` and `offscreen_smoke` tests must still pass
  when run opt-in.

## Gates

```sh
cargo fmt -p umicraft-render -- --check
cargo clippy -p umicraft-render --all-targets -- -D warnings
cargo test -p umicraft-render
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-render --no-deps
cargo test -p umicraft-render --test offscreen_terrain -- --ignored
cargo test -p umicraft-render --test offscreen_ui -- --ignored
```

Report the commit SHA, test count, the GPU adapter used, and any API that W3
must provide. Do not merge.
