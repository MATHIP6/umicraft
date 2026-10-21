# Threading model (target, spec §5.3)

Authoritative shape comes from the spec; this file tracks what exists per milestone.

Rules (restated):

1. Render/game threads never block on I/O, mods, or meshing.
2. All cross-thread communication uses bounded channels or snapshot buffers —
   no shared locks on hot paths.
3. Every cross-thread boundary is documented here as it is introduced.

## Current state (M3)

| Thread | Owns | Talks to |
|---|---|---|
| Main/render thread (winit) | Window, wgpu submission, the camera, the overlay, the mesh pool | Game thread via bounded frame and chunk channels; mesh pool via bounded job/result channels |
| **Game thread** (new in M3) | `umicraft_world::World`, `umicraft_game::GameState`, the 20 Hz tick, the HUD state | Network thread and render thread via bounded channels (events in, frames and chunk snapshots out) |
| Network thread (`umicraft-net`) | Tokio current-thread runtime, `Connection`, the play loop, the keepalive answers | Game thread via a bounded inbox of events; game thread via a bounded queue of serverbound commands |
| Mesh workers (1–4) | CPU meshing of immutable `ChunkSnapshot`s | Job receiver mutex (receive only) + bounded `sync_channel(512)` results |

The game thread is the only owner of the world: the render thread never
mutates a chunk, and the collision solver never takes a lock (spec §5.3,
ADR-0005).

### Network boundary

- `umicraft-client::network::spawn` starts one std thread named
  `umicraft-net` with a Tokio current-thread runtime. All socket I/O and
  packet framing stay on that runtime; no other thread ever awaits I/O.
- The play loop lives in the client crate (and not in `umicraft-net`) because
  the frozen `PlaySession` of that crate skips the M3 play packets
  (`Update Health` 0x06, `Held Item Change` 0x09, `Respawn` 0x07 and
  `Entity Teleport` 0x18), which the game thread needs. The framing, the
  login and the connection stay in `umicraft-net`.
- Decoded events cross the bounded `sync_channel(256)` inbox of the game
  thread. Chunk and bulk payloads are boxed; the queue bounds the queued
  world data at roughly 25 MiB worst case. When the inbox is full, the
  network thread blocks in `send` and TCP backpressure slows the server —
  memory never grows without bound.
- The serverbound packets the game thread produces cross a bounded
  `tokio::sync::mpsc` queue of 64 commands (game thread → network thread).
  The queue is polled with `try_send` by the game thread: a full queue keeps
  the command for the next tick instead of blocking the simulation. The input
  packet and the position report are *coalesced* — only the newest one of
  each is kept — because a backlog of them would burst on the socket, and the
  1.8 server counts the packets of a player (PandaSpigot kicks above 500
  packets in seven seconds). The commands that keep their order (the
  acknowledgement of a placement, the dig, the placement, the chat) are never
  dropped.
- The network thread selects between the socket and the command queue, so a
  command is written even while the server is silent. Keepalives are
  answered inside the play loop and never reach the game thread.

### Game thread boundary

- The game thread owns the `World` and the `GameState`. It applies the
  world packets, runs the 20 Hz tick, and publishes the results.
- Two bounded channels leave the game thread:
  - the **frames** (`sync_channel(32)`): the interpolated camera state, the
    HUD state and the session statistics. The render thread polls them with
    `try_recv`; the game thread publishes them with `try_send`, so a full
    queue drops the message instead of stalling the simulation (the HUD is
    published again when the last one was accepted, so a change is not lost).
  - the **chunk snapshots** (`sync_channel(64)`): the columns whose geometry
    changed. A snapshot that does not fit stays in the dirty set of the game
    thread and is derived again on a later tick, so the game thread never
    blocks on the renderer.
- The render thread interpolates between the two newest frames (GAM-8): it
  shows the simulation one tick behind and blends the two states with the
  fraction of a tick that elapsed since the newest one.

### Input boundary

- The render thread (or the headless script) turns the keyboard and the
  mouse into an `InputUpdate` and sends it into the bounded inbox of the
  game thread with `try_send`. A full inbox means the game thread is
  behind; the input is repeated on the next frame, and the look deltas are
  accumulated until they are accepted, so no mouse movement is lost.
- The game thread owns the view angles: the look deltas are applied to its
  yaw and pitch, and the server corrections overwrite them.

### World and mesh boundary

- `World` and all block mutation stay on the game thread. Columns are
  `Arc<ChunkColumn>` and updates are copy-on-write, so a worker never
  observes a half-applied column.
- A dirty column is snapshotted as a 3×3 `ChunkSnapshot` of `Arc` clones
  before it leaves the game thread. Snapshot capture is the only game
  thread work; the 16×256×16 meshing scan stays on workers.
- Jobs use `sync_channel(512)` and are submitted with `try_send`. If the
  queue is full, the chunk is put back on the render thread's waiting list
  and retried on a later frame; the render thread never waits for a worker.
- Results use `sync_channel(512)`. Workers may block briefly if the render
  thread is behind, but the render thread only polls (`try_recv`). On pool
  drop the job sender and result receiver are closed before joining, so a
  worker blocked on a full result queue exits instead of deadlocking.
- Workers share the job `Receiver` behind a `Mutex`, held only while
  `recv()` waits for the next job. The meshing inner loop and the terrain
  data are lock-free.
- A stale result is discarded when its column is no longer loaded.

### Current deviations from the target model

- Lighting workers, asset/IO pool, audio thread, and mod workers do not exist
  yet. Add each to this table when introduced.
- The player inventory is not received in M3: the 1.8 protocol only sends it
  with the window packets, which belong to M7. The client therefore knows
  the held item only from `Entity Equipment` (0x04) and the HUD shows the
  item of the selected slot.

## History

- M1: one Tokio runtime inside `Connection` owned all TCP I/O; higher layers
  awaited sequentially. `umicraft-testkit` fake servers ran as extra tasks on
  the test runtime; capture observation state was mutex-guarded at assertion
  time (cold path). Capture writing accumulated in memory and saved once.
- M2: dedicated network thread + bounded event channel; bounded mesh
  job/result queues with snapshot-based workers; offscreen screenshot path
  shares the GPU pipelines with the window path.
- M3: the world and the player state moved onto a game thread with a fixed
  20 Hz tick; the render thread only receives snapshots; the headless mode
  drives the same game thread as the windowed client.
