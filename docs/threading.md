# Threading model (target, spec §5.3)

Authoritative shape comes from the spec; this file tracks what exists per milestone.

Rules (restated):

1. Render/game threads never block on I/O, mods, or meshing.
2. All cross-thread communication uses bounded channels or snapshot buffers —
   no shared locks on hot paths.
3. Every cross-thread boundary is documented here as it is introduced.

## Current state (M2)

| Thread | Owns | Talks to |
|---|---|---|
| Main/render thread (winit) | Window, wgpu submission, `SessionWorld`, camera, input | Network thread via bounded `Receiver<NetEvent>`; mesh pool via bounded job/result channels |
| Network thread (`umicraft-net`) | Tokio current-thread runtime, `Connection`, `PlaySession`, keepalive replies | Main thread via bounded `sync_channel(256)` of `NetEvent` |
| Mesh workers (1–4) | CPU meshing of immutable `ChunkSnapshot`s | Job receiver mutex (receive only) + bounded `sync_channel(512)` results |

### Network boundary

- `umicraft-client::network::spawn` starts one std thread named
  `umicraft-net` with a Tokio current-thread runtime. All socket I/O and
  packet framing stay on that runtime; the main thread never awaits I/O.
- Decoded events (`JoinGame`, `Placement`, `Chat`, chunk/bulk/block updates,
  terminal `Error`) cross a bounded `std::sync::mpsc::sync_channel(256)`.
  Chunk and bulk payloads are boxed; the channel bounds queued world data at
  roughly 25 MiB worst case. When full, the network thread blocks in `send`
  and TCP backpressure slows the server — memory never grows without bound.
- The main thread only calls `try_recv` in the frame loop (screenshot mode
  uses `recv_timeout` against its collection deadline). Dropping the receiver
  makes the next send fail, which ends the thread.
- Keepalives are answered inside `PlaySession::next` on the network thread;
  the main thread never sees them.

### World and mesh boundary

- `SessionWorld`/`World` and all block mutation stay on the main thread.
  Columns are `Arc<ChunkColumn>` and updates are copy-on-write, so a worker
  never observes a half-applied column.
- A dirty column is snapshotted as a 3×3 `ChunkSnapshot` of `Arc` clones
  before submission. Snapshot capture is the only main-thread work; the
  16×256×16 meshing scan stays on workers.
- Jobs use `sync_channel(512)` and are submitted with `try_send`. If the
  queue is full, the chunk is put back on the dirty set and retried next
  frame; the render thread never waits for a worker.
- Results use `sync_channel(512)`. Workers may block briefly if the render
  thread is behind, but the render thread only polls (`try_recv`). On pool
  drop the job sender and result receiver are closed before joining, so a
  worker blocked on a full result queue exits instead of deadlocking.
- Workers share the job `Receiver` behind a `Mutex`, held only while
  `recv()` waits for the next job. The meshing inner loop and the terrain
  data are lock-free.
- A stale result is discarded when its column is no longer loaded. If the
  column was dirtied while meshing, the deferred position is re-queued after
  the result is consumed.

### Current deviations from the target model

- M2 still applies world packets and input on the render thread. The game
  thread with a fixed 20 TPS tick split is M3 work; the channel boundaries
  above already isolate network I/O and meshing.
- Lighting workers, asset/IO pool, audio thread, and mod workers do not exist
  yet. Add each to this table when introduced.

## History

- M1: one Tokio runtime inside `Connection` owned all TCP I/O; higher layers
  awaited sequentially. `umicraft-testkit` fake servers ran as extra tasks on
  the test runtime; capture observation state was mutex-guarded at assertion
  time (cold path). Capture writing accumulated in memory and saved once.
- M2: dedicated network thread + bounded event channel; bounded mesh
  job/result queues with snapshot-based workers; offscreen screenshot path
  shares the GPU pipelines with the window path.
