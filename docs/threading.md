# Threading model (target, spec §5.3)

Authoritative shape comes from the spec; this file tracks what exists per milestone.

Rules (restated):

1. Render/game threads never block on I/O, mods, or meshing.
2. All cross-thread communication uses bounded channels or snapshot buffers —
   no shared locks on hot paths.
3. Every cross-thread boundary is documented here as it is introduced.

## Current state (M6 + T1 biome tints)

| Thread | Owns | Talks to |
|---|---|---|
| Main/render thread (winit in windowed play) | Window, wgpu submission, the camera, the overlay, the UI frame and input translation, the sky/fog environment, the transparent draw order, the mesh pool, and the one-time official resource load | Game thread via bounded frame and chunk channels; mesh pool via bounded job/result channels |
| **Game thread** | `umicraft_world::World`, `umicraft_game::GameState`, the 20 Hz tick, the HUD state | Network thread and render thread via bounded channels (events in, frames and chunk snapshots out) |
| Network thread (`umicraft-net`) | Tokio current-thread runtime, `Connection`, `PlaySession` and the `drive` loop (ADR-0010), the keepalive answers | Game thread via a bounded inbox of events; game thread via a bounded queue of serverbound commands |
| Mesh workers (1–4) | CPU meshing of immutable `ChunkSnapshot`s, with a shared immutable `ModelMeshContext` (models, atlas layout and the optional `BiomeTints` table) when the official resources loaded | Job receiver mutex (receive only) + bounded `sync_channel(512)` results |

The game thread is the only owner of the world: the render thread never
mutates a chunk, and the collision solver never takes a lock (spec §5.3,
ADR-0005).

### Network boundary

- `umicraft-client::network::spawn` starts one std thread named
  `umicraft-net` with a Tokio current-thread runtime. All socket I/O and
  packet framing stay on that runtime; no other thread ever awaits I/O.
- After the login the thread hands the connection to
  `umicraft_net::play::drive`, the one play loop shared with the headless
  binary and the testkit (ADR-0010). `PlaySession` decodes the packets
  without I/O; `drive` writes its replies and the commands.
- Decoded `PlayEvent`s cross the bounded `sync_channel(256)` inbox of the
  game thread; a failure or a kick crosses it once as
  `GameEvent::Disconnected`. Chunk and bulk payloads are boxed; the queue bounds the queued
  world data at roughly 25 MiB worst case. When the inbox is full, the
  network thread blocks in `send` and TCP backpressure slows the server —
  memory never grows without bound.
- The serverbound packets the game thread produces cross a bounded
  `tokio::sync::mpsc` queue of 64 wire packets (game thread → network
  thread). The game thread reserves a slot with `try_reserve` before it
  converts a `ClientCommand` into its packet: a full queue keeps the command
  for the next tick instead of blocking the simulation. The input
  packet and the position report are *coalesced* — only the newest one of
  each is kept — because a backlog of them would burst on the socket, and the
  1.8 server counts the packets of a player (PandaSpigot kicks above 500
  packets in seven seconds). The commands that keep their order (the
  acknowledgement of a placement, the dig, the placement, the chat) are never
  dropped.
- `drive` selects between the socket read and the command queue, so a
  command is written even while the server is silent. Only those two
  cancel-safe futures race; every write (a keepalive answer, the entry
  packets, a command) happens after the `select!` released the connection,
  so a write is never cut in half by the other branch. Keepalives are
  answered there and never reach the game thread.
- `SessionCounters` (atomics: packets received and sent, keepalives
  answered) are written by the network thread and read by the overlay and
  the headless report; they take no lock.

### Game thread boundary

- The game thread owns the `World` and the `GameState`. It applies the
  world packets, runs the 20 Hz tick, and publishes the results.
- Two bounded channels leave the game thread:
  - the **frames** (`sync_channel(32)`): the interpolated camera state, the
    signed world time and dimension, the HUD state and the session statistics.
    The render thread polls them with `try_recv`; the game thread publishes
    them with `try_send`, so a full queue drops the message instead of stalling
    the simulation (the HUD is published again when the last one was accepted,
    so a change is not lost).
  - the **chunk snapshots** (`sync_channel(64)`): the columns whose geometry
    changed. A snapshot that does not fit stays in the dirty set of the game
    thread and is derived again on a later tick, so the game thread never
    blocks on the renderer.
- The render thread interpolates between the two newest frames (GAM-8): it
  shows the simulation one tick behind and blends the two states with the
  fraction of a tick that elapsed since the newest one.

### Clock and environment boundary (M6)

- `Time Update` (0x03) is decoded on the network thread and delivered as
  `NetEvent::TimeUpdate` through the existing bounded inbox. The game thread
  owns the signed `world_time` (a negative value means the server froze the
  cycle) and the current dimension; it never advances the clock locally.
- Each published frame carries `world_time` and `dimension` next to the
  interpolated camera state. The render thread keeps the newest values in
  `View` and builds one `RenderEnvironment` per frame with the same
  interpolation fraction the camera uses, then calls
  `Renderer::set_environment` once before rendering. The render thread never
  waits on the game thread for the clock.
- The renderer derives the sky pass, clear colour, fog and lightmap from that
  environment. Transparent ordering is render-thread state only: opaque chunks
  are submitted first, then each transparent half farthest-first by squared
  camera distance with `ChunkPos` as the deterministic tie-break. The sort uses
  one reused `Vec`; it is not a channel and takes no lock.

### Input boundary

- The render thread (or the headless script) turns the keyboard and the
  mouse into an `InputUpdate` and sends it into the bounded inbox of the
  game thread with `try_send`. A full inbox means the game thread is
  behind; the input is repeated on the next frame, and the look deltas are
  accumulated until they are accepted, so no mouse movement is lost.
- The game thread owns the view angles: the look deltas are applied to its
  yaw and pitch, and the server corrections overwrite them.

### UI boundary (M4)

- `umicraft-ui` is backend-agnostic: it owns no thread, no GPU resource and no
  I/O. The render thread builds one UI frame per rendered frame from the winit
  events (or the headless script), then hands the resulting `DrawList` to
  `umicraft-render` before the GPU submission.
- The UI never reads or mutates the world. It sees only the render snapshot
  (HUD state, frame statistics) and its own input, so it cannot block the game,
  network or mesh threads.
- The 1.8 font atlas is loaded once from the user's client jar when it is
  available; the original 3×5 fallback is used otherwise. The loader takes the
  version JSON, verifies the client jar, reads
  `assets/minecraft/textures/font/ascii.png` and decodes it. The atlas is
  immutable after load; a rejected atlas is retried once with the fallback
  before the renderer is allowed to fail.
- Opening the F2 screen clears game input, closes chat, releases the cursor
  grab and routes keyboard, text/IME, mouse buttons, wheel and modifier events
  to `UiInput`. The cursor position is tracked even while the UI is closed so
  the first click lands at the physical pointer. Closing the UI leaves the
  cursor uncaptured until the player clicks the game again.
- The windowed F2 screen and the headless `--ui-demo` drive the same pure
  `debug_screen`; the demo uses the same `UiInput` events and reads its state
  through the screen accessors. The draw list is the only output they share
  with the renderer. `--ui-demo --screenshot` reuses the same final draw list
  on the offscreen renderer.

### Official asset and model boundary (M5)

- The official resources are loaded once per process, on the main thread,
  before any mesh worker or renderer mesh exists. `ModelResources::load` first
  resolves and verifies the user's 1.8.8 client jar, reads the blockstates,
  block models and block/item textures in one bulk pass into an in-memory
  `ClientResources`, builds the immutable `BlockModelSet` from `definitions()`
  and the atlas, and wraps a `ModelMeshContext` in an `Arc`. The same bulk pass
  decodes the two optional 256×256 biome colormaps, and the main thread samples
  them into the immutable `BiomeTints` table placed in that context.
- The windowed client loads the bundle in `App::new`, before the mesh pool and
  before the event loop starts. Screenshot mode loads it before the pool and
  creates the offscreen renderer immediately, because the atlas must be
  accepted before any model job is queued. The load is start-up work only: no
  frame, tick or worker ever reads the jar or touches the disk for assets.
- Workers receive the `Arc<ModelMeshContext>` when the bundle loaded and call
  `mesh_chunk_with_models`; otherwise they call the legacy `mesh_chunk`. The
  context contains only immutable model quads, the atlas layout and the
  optional immutable `BiomeTints` table, so every worker reads it without a
  lock and no job mutates shared model or tint data. The colormap PNGs are
  dropped after sampling; only the 40-entry table is retained.
- The render thread uploads the texture array with `Renderer::set_block_atlas`
  after the renderer exists. If the upload is rejected (for example a low
  adapter limit), the bundle is dropped, the pending/waiting jobs and renderer
  meshes are cleared, and the pool is rebuilt without a context. This keeps
  real-layer meshes and the procedural fallback texture array from ever being
  mixed. A missing installation, corrupt jar, model error or rejection logs
  one warning; the game keeps running on the M2 procedural path.
- `--block-demo` stays on the main thread: it builds the synthetic columns,
  meshes one `ChunkSnapshot::from_columns` synchronously with
  `mesh_chunk_with_models`, prints the report, and only then creates the
  offscreen renderer when `--screenshot` is present. Without the flag it never
  touches the GPU; with it, the demo uses the same atlas upload as the window.

### World and mesh boundary

- `World` and all block mutation stay on the game thread. Columns are
  `Arc<ChunkColumn>` and updates are copy-on-write, so a worker never
  observes a half-applied column.
- A dirty column is snapshotted as a 3×3 `ChunkSnapshot` of `Arc` clones
  before it leaves the game thread. Snapshot capture is the only game
  thread work; the 16×256×16 meshing scan, model-quad emission and texture
  layer assignment stay on workers.
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
  with the window packets, which belong to M9. The client therefore knows
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
- M4: the pure `umicraft-ui` crate implements layout, focus, hit testing,
  widgets, the text field/list/button debug screen, the 1.8 font atlas and the
  draw list. The renderer gained a UI pass; the client builds the frame and
  translates winit input on the render thread. The font is loaded from the
  user's installation at startup and uploaded once, with the original fallback
  on every failure. `--ui-demo` drives the same screen without a server or
  GPU, and its screenshot path reuses the offscreen renderer.
- M5: the main thread loads the verified 1.8.8 client jar once before the pool
  and renderer, builds the immutable model set and atlas, and gives workers an
  `Arc<ModelMeshContext>`. Windowed and screenshot meshing switch to
  `mesh_chunk_with_models` as a unit with the atlas upload; a rejected atlas
  drops the bundle and rebuilds the pool without it. `--block-demo` meshes the
  synthetic acceptance scene synchronously and needs no server, and reaches
  the GPU only when a screenshot is requested.
- M6: the game thread stores the signed `Time Update` clock and dimension and
  publishes them in the frame channel. The render thread turns them into one
  `RenderEnvironment` per frame for the sky/fog/lightmap, and draws opaque
  terrain before a farthest-first transparent pass. Mesh workers now sample
  the world light nibbles and bake per-vertex smooth light/AO; the render
  thread still only owns uploaded meshes and the transparent order.
- T1 (post-M6 biome tints): the main-thread resource load also decodes the two
  optional colormaps and pre-samples the immutable 40-entry `BiomeTints` table
  into the shared `ModelMeshContext`. Workers average the 3×3 biome colours per
  tinted block and write the result into the new 44-byte vertex; the render
  thread uploads the same vertex layout. No new thread or channel was added,
  and the table is immutable so the workers still take no lock.
- ADR-0010 (play session): the client's own play loop was replaced by
  `umicraft_net::play::drive`, shared with the headless binary and the
  testkit. No thread or channel was added; the command queue now carries wire
  packets, and the loop writes only outside its `select!`.
