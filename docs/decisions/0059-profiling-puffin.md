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
