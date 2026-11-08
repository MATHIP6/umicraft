# 0058. F2 performance pass: start early, measure first, precision before speed

Status: accepted

## Context

The roadmap placed F2 (performance) after the parity sign-off (P13). Two
problems justify starting it now (owner, 2026-11-07):

- the frame rate is about the same as vanilla;
- the game looks less smooth far from the origin.

A code audit (`docs/tasks/f2/README.md`) found two separate causes:

- **Far from the origin, a precision bug.** The interpolated eye and the
  entity positions are cast to f32 before the renderer subtracts its integer
  origin, so they move in 0.0625–0.125-block steps around x = 1M. Terrain
  vertices are also absolute f32, so 1/16 model detail snaps above 2^19.
- **No fps lead, a structural cost.** The terrain is meshed, culled and drawn
  per 256-high column, with no occlusion culling. Every solid block goes
  through an alpha-test shader that disables early depth rejection. Vertices
  are 44 B. Some per-frame paths still create GPU buffers.

There are no instruments, so none of these costs has a number yet.

## Decision

1. F2 starts now, in parallel with the open owner validations of P9–P12. It
   follows the phases of `docs/tasks/f2/README.md`.
2. **Phase 0 (instruments) comes first.** No optimisation lands without a
   before/after number in `docs/perf-log.md`, from a criterion benchmark or
   the reproducible scene.
3. **Rendering keeps an f64 eye and an integer render origin end to end.**
   Everything sent to the GPU is relative to that origin, computed in f64
   before the cast to f32. This replaces the partial origin handling of the
   renderer.
4. Parity is not traded for speed: screenshot tests and `docs/parity.md` rows
   must stay green after every step.
5. The section-based meshing and visibility culling of phase 3, and the
   packed vertex format, each get their own ADR before code.

## Consequences

- P13 sign-off screenshots may need to be retaken once phase 3 lands. This is
  acceptable, because the pixels must not change.
- `RenderState`, `Camera` and the entity snapshots change type (f64 eye,
  origin-relative positions). This touches `umicraft-game`, `umicraft-client`
  and `umicraft-render` together.
- A profiling feature (puffin or tracy) adds an optional dependency, which
  needs a licence check in `deny.toml`.
