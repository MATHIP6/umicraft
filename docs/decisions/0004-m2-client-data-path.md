# ADR-0004 — M2 client data path: copy-on-write columns, snapshot meshing, bounded queues

- Status: Accepted
- Date: 2026-10-21
- Deciders: project owner

## Context

M2 adds the first data-heavy path: a 1.8 server sends a bulk of chunk columns
(169 columns for the test server's `view-distance=6`), then streams block
changes and re-sends columns as players move. The client must turn that into
triangles without stalling the network socket or the frame loop.

Three constraints shape the design:

- spec §5.3 requires render/game threads never to block on I/O or meshing,
  and cross-thread hand-offs to be bounded;
- `umicraft-world` stores paletted sections behind cheap shared handles;
- the same renderer must serve a winit window and the headless screenshot
  path used for acceptance.

## Options

1. Mesh synchronously on the main thread after every chunk packet. Simple,
   but a single 16×256×16 scan takes ~0.45–0.7 ms in release, so a bulk of
   169 columns visibly stalls the first frames.
2. Share the live `World` behind a lock with worker threads. Rejected:
   workers would contend with packet application and block the render thread
   on the world mutex.
3. Copy-on-write columns + immutable 3×3 snapshots + bounded worker queues.
   *(chosen)*

## Decision

- **World ownership.** `World` is owned by the main thread. Columns live in
  `Arc<ChunkColumn>`; block updates use `Arc::make_mut`, so a mesher keeps a
  consistent old version while the live world advances. No world lock exists.
- **Snapshots.** Before submitting a dirty column, the main thread captures
  the centre plus its eight neighbours (`ChunkSnapshot`). Snapshots hold nine
  `Arc` clones and are immutable for the job's lifetime.
- **Mesh workers.** A fixed pool (up to four workers, leaving cores for the
  render and network threads) receives jobs through a bounded
  `std::sync::mpsc::sync_channel(512)`.
  Workers share the receiver behind a mutex that is held only while waiting
  for the next job; the meshing inner loop is lock-free. Results return on a
  bounded `sync_channel(512)`. The render thread only calls `try_send` and
  `try_recv`, so a full queue leaves the chunk dirty and retries next frame.
- **Network hand-off.** A dedicated thread runs a Tokio current-thread
  runtime and forwards decoded play events over a bounded
  `sync_channel(256)`. When the queue is full, the network thread blocks and
  TCP backpressure does the rest; the render thread never waits on the
  socket. Chunk/bulk payloads are boxed so a queued event stays pointer-sized
  apart from the payload itself.
- **Rendering.** One `Renderer` owns the wgpu device/queue and either a
  surface target (window) or an offscreen colour/depth target (screenshot).
  Both paths share the same terrain and overlay pipelines, which differ only
  in render-target format. Screenshot mode renders one frame, reads it back
  (respecting WebGPU's 256-byte row padding), and writes an RGBA PNG.
- **Assets.** Textures and the bitmap font are generated procedurally in
  code (original assets), indexed by the fixed layer table in
  `umicraft-mesh`. Real resource-pack loading arrives with `umicraft-assets`.

## Consequences

- The main thread never blocks on meshing or socket reads; frame time during
  bulk load is bounded by snapshot capture and GPU upload, not meshing.
- A stale result can arrive after its column was unloaded or re-dirtied. The
  client discards results for unloaded chunks and re-dirties the column when
  a newer version is pending.
- The current M2 build still applies world packets on the render thread;
  splitting the game tick onto its own thread (spec §5.3) is M3 work. The
  channel boundaries are already in the shape that split needs.
- Screenshot acceptance is deterministic and does not need a display server,
  so it can run in CI. The windowed path is smoke-tested separately.
- Queue capacities are a memory/latency trade-off and are documented in
  `docs/threading.md`; they should be profiled again when streaming chunks
  during movement lands.
