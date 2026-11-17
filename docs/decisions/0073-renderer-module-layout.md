# ADR-0073 — Layout of the renderer module

Status: accepted (2026-11-16)

## Context

`umicraft-render/src/renderer.rs` was 4 255 lines, most of it one
3 085-line `impl Renderer` with about 120 methods. `Renderer::create` was
601 lines of inline wgpu descriptors, with the camera buffer, the camera
bind group and the texture layout copied several times, and `draw` was a
356-line function. Finding where a pass is drawn or a texture is uploaded
meant searching the whole file, as with `game.rs` before ADR-0072.

## Decision

`renderer` is a directory module. `mod.rs` keeps the `Renderer` struct, the
constants and uniforms the other passes share, and re-exports the crate
paths into it. Each file holds one `impl Renderer` block, by concern:

| Module | Holds |
|---|---|
| `init` | `new_windowed`, `new_offscreen` and `create` |
| `resources` | the start-up resources: first textures, samplers, layouts, camera uniforms |
| `scene` | the `set_*` calls that describe the next frame |
| `frame` | the frame entry points, the camera uniforms, the submission |
| `draw` | the passes of a frame, in order; each longer step is a named method |
| `terrain` | chunk meshes, culling and the draw lists |
| `textures`, `icons`, `entities` | uploads of their resources |
| `readback`, `surface`, `stats` | pixels and screenshots; adapter and surface; debug counters |
| `pipelines` | the terrain and overlay pipelines and the shared GPU mesh helpers |

Rules for code added later: a new method goes into the file of its
concern; a resource created once at start-up gets a named builder in
`resources` rather than an inline descriptor in `create`; a new pass gets
its own method called from `draw` in its place in the frame order.

## Consequences

- The split and the extractions changed no call order and no descriptor:
  the unit tests are unchanged and the GPU tests give the same results as
  before (70 pass; the 6 terrain tests that already failed still fail, see
  below).
- `Renderer` still has about 130 fields and `create` fills them in one
  literal. Grouping them (as `Game` was in ADR-0072) and the `SceneParams`
  idea (one value per frame instead of the `set_*` calls) change the public
  API and are left for their own decision.
- `prepare_visible_chunks` (about 200 lines) runs every frame; it is left
  whole until a change to it comes with a benchmark (docs/perf-log.md).
- Six GPU tests in `tests/offscreen_terrain.rs` and `tests/offscreen_fog.rs`
  fail on `main` before this change: the terrain quad they draw is not in
  the picture, which shows only the sky colour.
