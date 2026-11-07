# 0059. CPU profiling with puffin behind a cargo feature

Status: accepted

## Context

F2 (ADR-0058) needs CPU spans on the frame, the world sync, the visibility
pass, uploads, mesh jobs and the game tick, with zero cost in normal builds.

## Decision

- `puffin` 0.20 and `puffin_http` 0.17 (both MIT OR Apache-2.0, allowed by
  `deny.toml`; `cargo deny check licenses` passes) are optional dependencies of
  `umicraft-core`, enabled by the `profiling` feature. The feature is
  forwarded by `umicraft-client`, `umicraft-game` and `umicraft-render`.
- `umicraft_core::profile_scope!("name")` opens a span; without the feature it
  expands to nothing. `profiling::new_frame()` splits frames and
  `profiling::serve()` streams to `127.0.0.1:8585` (the `puffin_viewer`
  default).
- puffin was preferred to tracy: pure Rust, no C++ build, no external server
  to install beyond `cargo install puffin_viewer`.

## Use

    cargo run -p umicraft-client --features profiling --release -- <args>
    puffin_viewer            # connects to 127.0.0.1:8585

Spans: `redraw`, `sync_world`, `WorldView::sync`, `prepare_visible_chunks`,
`upload_mesh`, `render_frame` (render thread), `mesh job` (workers) and
`game tick` (game thread).

## GPU timestamps (F2-0b)

The frame is a single render pass, so the groups are timed with
`RenderPass::write_timestamp` (features `TIMESTAMP_QUERY`,
`TIMESTAMP_QUERY_INSIDE_ENCODERS`, `TIMESTAMP_QUERY_INSIDE_PASSES`, requested
only when the adapter offers all three). Six stamps delimit five groups: sky
(sky + panorama), opaque (terrain + moving blocks), entities (shadows to
dropped items), translucent (particles, weather, transparent terrain, clouds,
border, cracks, outline) and ui (hand, screen overlays, HUD). Results are
resolved into a ring of three readback buffers and polled without blocking, so
they are two to three frames old; a frame is skipped when all slots are in
flight. F3 shows `GPU <total> ms: ...`, or `GPU time: n/a` without support.
`gpu_timer.rs` in `umicraft-render`; test `offscreen_gpu_timer` (`--ignored`).

## Benchmark scene and release profile (F2-0c, F2-0d)

`umicraft-client --bench-scene [--bench-origin N] [--bench-frames N]` draws a
fixed synthetic world along a scripted camera path offscreen and prints the
frame time (mean, p50, p95, p99, max) and the GPU time per group. The terrain
depends only on the position relative to the origin chunk, so two origins draw
the same geometry. `lto = "thin"` with `codegen-units = 1` was measured and
**not kept** (no gain, 5.7× slower release build); numbers in `docs/perf-log.md`.
